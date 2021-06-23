use futures::{executor::block_on, Future};
use std::sync::{Arc, Mutex};
use tokio::sync::watch::{self, Receiver, Sender};

#[derive(Clone, Debug)]
pub struct ServiceContext {
    status: Box<ServiceStatus>,
    rx: Receiver<()>,
    rx2: Receiver<ServiceControlRequest>,
}

impl ServiceContext {
    pub fn wait(&mut self) -> std::io::Result<()> {
        match block_on(self.rx.changed()) {
            Ok(_) => Ok(()),
            Err(err) => Err(std::io::Error::new(std::io::ErrorKind::Other, err)),
        }
    }
}

#[derive(Clone)]
pub enum ServiceControlCode {
    Stop(),
    Pause(),
    Resume(),
}

#[derive(Clone, Debug)]
pub enum ServiceStatus {
    Stopped(),
    Paused(ServiceContext),
    Running(),
}

#[derive(Clone, Debug)]
pub enum ServiceControlRequest {
    Init(),
    Stop(),
    Pause(),
    Continue(),
}

pub struct ServiceInstance {
    context: Arc<Mutex<ServiceContext>>,
}

pub enum Event<T> {
    StatusChanged(ServiceStatus),
    Future(T),
}

impl ServiceInstance {
    pub fn stopped(&self) -> bool {
        matches!(
            *self.context.lock().unwrap().status,
            ServiceStatus::Stopped()
        )
    }

    pub fn paused(&self) -> bool {
        matches!(
            *self.context.lock().unwrap().status,
            ServiceStatus::Paused(_)
        )
    }

    pub fn is_running(&self) -> bool {
        matches!(
            *self.context.lock().unwrap().status,
            ServiceStatus::Running()
        )
    }

    pub async fn wait_async(&self) -> std::io::Result<ServiceStatus> {
        let mut rx = self.context.lock().unwrap().rx2.clone();
        match self.context.lock() {
            Ok(context) => match &*context.status {
                ServiceStatus::Stopped() => {
                    return Ok(ServiceStatus::Stopped());
                }
                ServiceStatus::Paused(context) => {
                    return Ok(ServiceStatus::Paused(context.clone()));
                }
                ServiceStatus::Running() => {}
            },
            Err(_) => {}
        }
        match rx.changed().await {
            Ok(_) => match rx.borrow().clone() {
                ServiceControlRequest::Stop() => Ok(ServiceStatus::Stopped()),
                ServiceControlRequest::Pause() => {
                    Ok(ServiceStatus::Paused(self.context.lock().unwrap().clone()))
                }
                ServiceControlRequest::Continue() => Ok(ServiceStatus::Running()),
                _ => unreachable!(),
            },
            Err(err) => Err(std::io::Error::new(std::io::ErrorKind::Other, err)),
        }
    }

    pub fn wait(&self) -> std::io::Result<ServiceStatus> {
        match block_on(self.wait_async()) {
            Ok(status) => Ok(status),
            Err(err) => Err(err),
        }
    }

    pub fn wait_future<T: futures::Future>(
        &self,
        future: T,
    ) -> std::io::Result<Event<<T as Future>::Output>> {
        block_on(async {
            tokio::select! {
                o = future => Ok(Event::Future(o)),
                s = self.wait_async() => {
                    match s {
                        Ok(s) => Ok(Event::StatusChanged(s)),
                        Err(err) => Err(err),
                    }
                }
            }
        })
    }
}

pub struct Service {
    status: ServiceStatus,
    context: Arc<Mutex<ServiceContext>>,
    notify: Box<Sender<()>>,
    channel: (
        Box<Sender<ServiceControlRequest>>,
        Receiver<ServiceControlRequest>,
    ),
}

impl Drop for Service {
    fn drop(&mut self) {
        self.stop().unwrap();
    }
}

impl Service {
    pub fn new() -> (Self, Arc<ServiceInstance>) {
        let (tx, rx) = tokio::sync::watch::channel(());
        let (tx1, rx1) = watch::channel(ServiceControlRequest::Init());
        let context = Arc::new(Mutex::new(ServiceContext {
            status: Box::new(ServiceStatus::Running()),
            rx,
            rx2: rx1.clone(),
        }));
        (
            Service {
                status: ServiceStatus::Running(),
                notify: Box::new(tx),
                context: context.clone(),
                channel: (Box::new(tx1), rx1.clone()),
            },
            Arc::new(ServiceInstance {
                context: context.clone(),
            }),
        )
    }

    pub fn stop(
        &mut self,
    ) -> Result<ServiceStatus, watch::error::SendError<ServiceControlRequest>> {
        match self.status {
            ServiceStatus::Stopped() => Ok(self.status.clone()),
            _ => self.send(ServiceControlCode::Stop()),
        }
    }

    pub fn pause(
        &mut self,
    ) -> Result<ServiceStatus, watch::error::SendError<ServiceControlRequest>> {
        match self.status {
            ServiceStatus::Running() => self.send(ServiceControlCode::Pause()),
            _ => Ok(self.status.clone()),
        }
    }

    pub fn resume(
        &mut self,
    ) -> Result<ServiceStatus, watch::error::SendError<ServiceControlRequest>> {
        match self.status {
            ServiceStatus::Paused(_) => self.send(ServiceControlCode::Resume()),
            _ => Ok(self.status.clone()),
        }
    }

    pub fn send(
        &mut self,
        code: ServiceControlCode,
    ) -> Result<ServiceStatus, watch::error::SendError<ServiceControlRequest>> {
        let res = self.channel.0.send(match code {
            ServiceControlCode::Stop() => {
                self.context.lock().unwrap().status = Box::new(ServiceStatus::Stopped());
                self.status = ServiceStatus::Stopped();
                ServiceControlRequest::Stop()
            }
            ServiceControlCode::Pause() => {
                let mut ctx = self.context.lock().unwrap();
                ctx.status = Box::new(ServiceStatus::Paused(ctx.clone()));
                self.status = ServiceStatus::Paused(ctx.clone());
                ServiceControlRequest::Pause()
            }
            ServiceControlCode::Resume() => {
                self.notify.send(()).unwrap();
                let (tx, rx) = tokio::sync::watch::channel(());
                self.notify = Box::new(tx);

                let mut ctx = self.context.lock().unwrap();
                ctx.rx = rx.clone();
                ctx.status = Box::new(ServiceStatus::Running());
                self.status = ServiceStatus::Running();
                ServiceControlRequest::Continue()
            }
        });

        let (tx, rx) = tokio::sync::watch::channel(ServiceControlRequest::Init());
        self.channel = (Box::new(tx), rx);
        self.context.lock().unwrap().rx2 = self.channel.1.clone();

        match res {
            Ok(_) => Ok(self.status.clone()),
            Err(err) => Err(err),
        }
    }
}
