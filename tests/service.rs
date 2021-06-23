#[cfg(test)]
mod tests {
    use service_rs::service::{self, ServiceStatus};
    use std::{
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        },
        thread,
        time::Duration,
    };

    #[test]
    fn wait_test() {
        panic_after(Duration::from_millis(500), || {
            let (mut svc, inst) = service::Service::new();
            let stopped = Arc::new(AtomicBool::new(false));
            let stopped2 = stopped.clone();
            let handle = thread::spawn(move || loop {
                match inst.wait() {
                    Ok(status) => match status {
                        ServiceStatus::Stopped() => {
                            stopped2.store(true, Ordering::Relaxed);
                            return;
                        }
                        ServiceStatus::Paused(_) => unreachable!(),
                        ServiceStatus::Running() => unreachable!(),
                    },
                    Err(_) => {}
                }
                todo!()
            });
            assert!(matches!(svc.stop(), Ok(status) if matches!(status, ServiceStatus::Stopped())));
            assert!(matches!(handle.join(), Ok(_)));
            assert!(stopped.load(Ordering::Relaxed));
        });
    }

    #[tokio::test]
    async fn wait_async_test() {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();
        let (mut svc, inst) = service::Service::new();
        let stopped = Arc::new(AtomicBool::new(false));
        let stopped2 = stopped.clone();
        let handle = rt.spawn(async move {
            loop {
                match tokio::select! {
                    f = inst.wait_async() => f,
                    _ = tokio::time::sleep(Duration::from_millis(10)) => {
                        Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "timeout"))
                    }
                } {
                    Ok(status) => match status {
                        ServiceStatus::Stopped() => {
                            stopped2.store(true, Ordering::Relaxed);
                            return Ok(());
                        }
                        ServiceStatus::Paused(_) => unreachable!(),
                        ServiceStatus::Running() => unreachable!(),
                    },
                    Err(err) => {
                        return Err(err);
                    }
                }
                // TODO
            }
        });
        std::thread::sleep(Duration::from_secs(1));
        assert!(matches!(svc.stop(), Ok(status) if matches!(status, ServiceStatus::Stopped())));
        assert!(matches!(handle.await, Ok(res) if res.is_err()));
        assert!(!stopped.load(Ordering::Relaxed));
        rt.shutdown_background();
    }

    #[tokio::test]
    async fn wait_async_timeout_test() {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();
        let (mut svc, inst) = service::Service::new();
        let stopped = Arc::new(AtomicBool::new(false));
        let stopped2 = stopped.clone();
        let handle = rt.spawn(async move {
            loop {
                match tokio::select! {
                    f = inst.wait_async() => f,
                    _ = tokio::time::sleep(Duration::from_secs(100)) => {
                        Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "timeout"))
                    }
                } {
                    Ok(status) => match status {
                        ServiceStatus::Stopped() => {
                            stopped2.store(true, Ordering::Relaxed);
                            return Ok(());
                        }
                        ServiceStatus::Paused(_) => unreachable!(),
                        ServiceStatus::Running() => unreachable!(),
                    },
                    Err(err) => {
                        return Err(err);
                    }
                }

                // TODO
            }
        });
        assert!(matches!(svc.stop(), Ok(status) if matches!(status, ServiceStatus::Stopped())));
        assert!(matches!(handle.await, Ok(_)));
        assert!(stopped.load(Ordering::Relaxed));

        rt.shutdown_background();
    }

    #[test]
    fn inst_test() {
        panic_after(Duration::from_millis(500), || {
            let (mut svc, inst) = service::Service::new();
            let stopped = Arc::new(AtomicBool::new(false));
            let stopped2 = stopped.clone();
            let inst2 = inst.clone();
            let handle = thread::spawn(move || loop {
                match inst2.wait() {
                    Ok(status) => match status {
                        service::ServiceStatus::Stopped() => {
                            stopped2.store(true, Ordering::Relaxed);
                            return;
                        }
                        service::ServiceStatus::Paused(_) => {}
                        service::ServiceStatus::Running() => {}
                    },
                    Err(_) => {}
                }
            });

            assert!(inst.is_running());
            assert!(matches!(svc.stop(), Ok(status) if matches!(status, ServiceStatus::Stopped())));
            assert!(inst.stopped());

            assert!(matches!(handle.join(), Ok(_)));
            assert!(stopped.load(Ordering::Relaxed));
        });
    }

    #[test]
    fn pause_resume_test() {
        panic_after(Duration::from_millis(500), || {
            let (mut svc, inst) = service::Service::new();
            let stopped = Arc::new(AtomicBool::new(false));
            let stopped2 = stopped.clone();
            let inst2 = inst.clone();
            let handle = thread::spawn(move || loop {
                match inst2.wait() {
                    Ok(status) => match status {
                        service::ServiceStatus::Stopped() => {
                            stopped2.store(true, Ordering::Relaxed);
                            return;
                        }
                        service::ServiceStatus::Paused(mut ctx) => {
                            assert!(matches!(ctx.wait(), Ok(())));
                        }
                        service::ServiceStatus::Running() => {}
                    },
                    Err(_) => {}
                }
            });
            assert!(inst.is_running());
            assert!(
                matches!(svc.pause(), Ok(status) if matches!(status, ServiceStatus::Paused(_)))
            );
            assert!(inst.paused());

            assert!(
                matches!(svc.resume(), Ok(status) if matches!(status, ServiceStatus::Running()))
            );
            assert!(inst.is_running());

            assert!(matches!(svc.stop(), Ok(status) if matches!(status, ServiceStatus::Stopped())));
            assert!(inst.stopped());

            assert!(matches!(handle.join(), Ok(_)));
            assert!(stopped.load(Ordering::Relaxed));
        });
    }

    #[test]
    fn wait_future_test() {
        panic_after(Duration::from_secs(3), || {
            let (mut svc, inst) = service::Service::new();
            let stopped = Arc::new(AtomicBool::new(false));
            let stopped2 = stopped.clone();
            let handle = thread::spawn(move || loop {
                match inst.wait_future(async { std::thread::sleep(Duration::from_millis(10)) }) {
                    Ok(event) => match event {
                        service::Event::StatusChanged(status) => match status {
                            service::ServiceStatus::Stopped() => {
                                stopped2.store(true, Ordering::Relaxed);
                                return Ok(1);
                            }
                            service::ServiceStatus::Paused(_) => unreachable!(),
                            service::ServiceStatus::Running() => unreachable!(),
                        },
                        service::Event::Future(_) => {
                            return Ok(2);
                        }
                    },
                    Err(err) => {
                        return Err(err);
                    }
                }
            });
            thread::sleep(Duration::from_secs(1));
            assert!(matches!(svc.stop(), Ok(status) if matches!(status, ServiceStatus::Stopped())));
            assert!(matches!(handle.join(), Ok(res) if matches!(res, Ok(res) if res == 2)));
            assert!(!stopped.load(Ordering::Relaxed));
        });
    }

    fn panic_after<T, F>(d: Duration, f: F) -> T
    where
        T: Send + 'static,
        F: FnOnce() -> T,
        F: Send + 'static,
    {
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let handle = std::thread::spawn(move || {
            let val = f();
            done_tx.send(()).expect("Unable to send completion signal");
            val
        });
        match done_rx.recv_timeout(d) {
            Ok(_) => handle.join().expect("Thread panicked"),
            Err(_) => panic!("Thread took too long"),
        }
    }
}
