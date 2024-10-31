use std::{
    future::Future,
    sync::{Arc, Condvar, Mutex},
    task::{Context, Poll},
};

use cooked_waker::{IntoWaker, WakeRef};
use futures::pin_mut;

#[derive(Debug, Default)]
struct Notifier {
    was_notified: Mutex<bool>,
    cv: Condvar,
}

impl Notifier {
    fn wait(&self) {
        let mut was_notified = self.was_notified.lock().unwrap();

        while !*was_notified {
            log::info!("Waiting for wake");
            was_notified = self.cv.wait(was_notified).unwrap();
        }

        *was_notified = false;
    }
}

impl WakeRef for Notifier {
    fn wake_by_ref(&self) {
        let was_notified = self
            .was_notified
            .lock()
            .map(|mut b| {
                let ret = *b;
                *b = true;

                ret
            })
            .unwrap();

        log::info!("Notifier with {}", was_notified);
        if !was_notified {
            log::info!("Notifier Awoken");
            self.cv.notify_one();
        }
    }
}
pub fn run_future<F: Future>(future: F) -> F::Output {
    pin_mut!(future);

    let notifier = Arc::new(Notifier::default());
    let waker = notifier.clone().into_waker();

    let mut cx = Context::from_waker(&waker);

    log::trace!("Run Fut: cx: {:?} Notifier: {:?}", cx, notifier);
    let output = loop {
        log::trace!("Looping Fut: cx: {:?}", cx);
        match future.as_mut().poll(&mut cx) {
            Poll::Ready(output) => {
                log::trace!("Looping Fut: Breaking out Ready, cx: {:?}", cx);
                break output;
            }
            Poll::Pending => {
                log::trace!("Looping Fut: Pending will wait, cx: {:?}", cx);
                notifier.wait();
                log::trace!("Looping Fut: Pending will continue, cx: {:?}", cx);
            }
        };
    };

    output
}
