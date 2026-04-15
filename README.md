![Rust](https://img.shields.io/badge/Rust-stable-orange?logo=rust)
![Windows](https://img.shields.io/badge/Windows-Kernel%20Driver-blue?logo=windows)
![Research](https://img.shields.io/badge/purpose-security%20research-red)
![License](https://img.shields.io/badge/license-MIT-green)

# ⚡ EBKiller – Kernel Process Terminator (BYOVD)

> ⚠️ For educational and security research purposes only

As of 2026-04-15, the driver is not listed on LOLDDrivers or in Microsoft's recommended driver block rules.

**EBKiller** is a Rust-based tool that demonstrates a **Bring Your Own Vulnerable Driver (BYOVD)** technique by leveraging a **Microsoft WHQL-signed driver (`eb.sys`)** (SHA256:97bd65e98cdc4e93d49edd4ea905d43a61244df0fd3323e6649330de3b1be091) to terminate processes from kernel mode.

---

## 🚀 Features

* Load and manage a **WHQL-signed driver** (`eb.sys`)
* Communicate with the driver via `DeviceIoControl`
* Monitor and terminate a process by name
* Continuous process scanning loop
* Graceful shutdown via `Ctrl+C`
* Safe handle management (RAII pattern)

---

## 🧠 What is BYOVD?

**BYOVD (Bring Your Own Vulnerable Driver)** is a technique where a legitimate, signed driver is abused to perform privileged kernel operations.

In this project:

* The driver is **legitimately signed (WHQL)**
* The attack surface comes from **exposed IOCTL functionality**
* The tool leverages it for **kernel-level process termination**

---

## ⚙️ How It Works

1. The program installs (or opens) the `eb` driver as a Windows service
2. Starts the driver
3. Continuously scans running processes
4. When the target process is found:

   * Sends an IOCTL request to the driver
   * The driver executes the privileged operation
5. On exit:

   * Stops the driver
   * Deletes the service

---

## 📦 Requirements

* Windows OS
* Administrator privileges
* `Kill.sys` (WHQL-signed driver) in the same directory

---

## 🛠️ Usage

```bash
cargo run -- -n notepad.exe
```

### Arguments

| Flag         | Description         |
| ------------ | ------------------- |
| `-n, --name` | Target process name |

---

## 📁 Project Structure

```
.
├── src/
│   └── main.rs
├── Kill.sys
├── Cargo.toml
└── README.md
```

---

## ⚠️ Disclaimer

This project is intended strictly for:

* Security research
* Educational purposes
* Controlled lab environments

Unauthorized use on systems you do not own or have permission to test is strictly discouraged.

---

## 📜 License

MIT License
