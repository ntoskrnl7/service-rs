use futures::{executor::block_on, Future};
use log::error;
use std::{
    error::Error,
    fmt,
    sync::{Arc, Mutex},
};
use tokio::sync::watch::{self, error::RecvError, Receiver, Sender};

#[derive(Clone, Debug)]
pub struct ServiceContext {
    status: Box<ServiceStatus>,
    rx: Receiver<()>,
    rx2: Receiver<ServiceControlRequest>,
}

impl ServiceContext {
    pub fn wait(&mut self) {
        match block_on(self.rx.changed()) {
            Ok(_) => {}
            Err(err) => {
                error!("Failed to wait : {}", err)
            }
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
    Pause(ServiceContext),
    Continue(),
}

#[derive(Debug)]
pub struct ServiceError {
    cause: Option<&'static dyn Error>,
}

impl Error for ServiceError {}

impl fmt::Display for ServiceError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if self.cause.is_some() {
            f.write_fmt(format_args!("{:?}", self.cause))
        } else {
            write!(f, "unknown service error")
        }
    }
}

pub struct ServiceInstance {
    context: Arc<Mutex<ServiceContext>>,
}

pub enum Event<T> {
    ServiceStatus(ServiceStatus),
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

    pub fn do_events<T: futures::Future>(
        &self,
        future: T,
    ) -> Result<Event<<T as Future>::Output>, RecvError> {
        let mut rx = self.context.lock().unwrap().rx2.clone();

        match self.context.lock() {
            Ok(context) => match &*context.status {
                ServiceStatus::Stopped() => {
                    return Ok(Event::ServiceStatus(ServiceStatus::Stopped()));
                }
                ServiceStatus::Paused(context) => {
                    return Ok(Event::ServiceStatus(ServiceStatus::Paused(context.clone())));
                }
                ServiceStatus::Running() => {}
            },
            Err(_) => {}
        }

        enum Event2<T> {
            ServiceControlRequest(ServiceControlRequest),
            Future(T),
        }

        block_on(async {
            match tokio::select! {
                o = future => Ok(Event2::Future(o)),
                c = rx.changed() => {
                    match c {
                        Ok(_) => Ok(Event2::ServiceControlRequest(rx.borrow().clone())),
                        Err(err) => Err(err),
                    }
                },
            } {
                Ok(event) => match event {
                    Event2::ServiceControlRequest(r) => match r {
                        ServiceControlRequest::Stop() => {
                            Ok(Event::ServiceStatus(ServiceStatus::Stopped()))
                        }
                        ServiceControlRequest::Pause(_) => Ok(Event::ServiceStatus(
                            ServiceStatus::Paused(self.context.lock().unwrap().clone()),
                        )),
                        _ => Ok(Event::ServiceStatus(ServiceStatus::Running())),
                    },
                    Event2::Future(out) => Ok(Event::Future(out)),
                },
                Err(err) => Err(err),
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

        let s = Service {
            status: ServiceStatus::Running(),
            notify: Box::new(tx),
            context: context.clone(),
            channel: (Box::new(tx1), rx1.clone()),
        };

        (
            s,
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
                ServiceControlRequest::Pause(ctx.clone())
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
