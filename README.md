# service-rs

[![Rust](https://github.com/ntoskrnl7/service-rs/actions/workflows/rust.yml/badge.svg)](https://github.com/ntoskrnl7/service-rs/actions/workflows/rust.yml)

A library for implementing programs that support pause, stop, and resume.

## Overview

It helps to implement services with interfaces similar to windows services.

| Status          | Available control methods | Result  |
| --------------- | ------------------------- | ------- |
| Running, Paused | Stop                      | Stopped |
| Running         | Pause                     | Paused  |
| Paused          | Resume                    | Running |

## Example

```rust
use service_rs::service;
use std::{io::BufRead, thread};

let (mut svc, inst) = service::Service::new();
thread::spawn(move || loop {
    match inst.wait() {
        Ok(status) => match status {
            ServiceStatus::Stopped() => {
                return;
            }
            ServiceStatus::Paused(mut ctx) => {
                ctx.wait().unwrap();
            },
            ServiceStatus::Running() => {},
        },
        Err(_) => {}
    }
    
    println!("TODO");
});

let mut available_cmd = "[P: Pause, S: Stop]";
loop {
    println!("{}", ["Selet a command", available_cmd].join(" "));
    let mut line = String::new();
    let _ = std::io::stdin().lock().read_line(&mut line);
    let line = line.to_uppercase();
    if !line.is_empty() {
        match match &line[0..1] {
            "P" => svc.pause(),
            "R" => svc.resume(),
            "S" => svc.stop(),
            _ => {
                println!("Uknown command : {}", line);
                continue;
            }
        } {
            Ok(status) => match status {
                service::ServiceStatus::Stopped() => {
                    break;
                }
                service::ServiceStatus::Paused(_) => {
                    available_cmd = "[R: Resume, S: Stop]";
                }
                service::ServiceStatus::Running() => {
                    available_cmd = "[P: Pause, S: Stop]";
                }
            },
            Err(err) => {
                println!("Failed to service command : {}", err);
                continue;
            }
        }
    }
}
```
