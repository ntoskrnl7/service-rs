#[cfg(test)]
mod tests {
    use service_rs::service::{self, ServiceStatus};
    use std::{
        sync::{
            atomic::{AtomicBool, Ordering},
            mpsc, Arc,
        },
        thread,
        time::Duration,
    };

    #[test]
    fn basic_test() {
        panic_after(Duration::from_millis(500), || {
            let (mut svc, inst) = service::Service::new();
            let stopped = Arc::new(AtomicBool::new(false));
            let stopped2 = stopped.clone();
            let handle = thread::spawn(move || loop {
                match inst.do_events(async {}) {
                    Ok(event) => match event {
                        service::Event::ServiceStatus(status) => match status {
                            service::ServiceStatus::Stopped() => {
                                stopped2.store(true, Ordering::Relaxed);
                                return;
                            }
                            service::ServiceStatus::Paused(_) => {}
                            service::ServiceStatus::None() => {}
                        },
                        service::Event::Future(_) => {}
                    },
                    Err(_) => {}
                }
            });
            assert!(matches!(svc.stop(), Ok(status) if matches!(status, ServiceStatus::Stopped())));
            assert!(matches!(handle.join(), Ok(_)));
            assert_eq!(stopped.load(Ordering::Relaxed), true);
        });
    }

    fn panic_after<T, F>(d: Duration, f: F) -> T
    where
        T: Send + 'static,
        F: FnOnce() -> T,
        F: Send + 'static,
    {
        let (done_tx, done_rx) = mpsc::channel();
        let handle = thread::spawn(move || {
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
