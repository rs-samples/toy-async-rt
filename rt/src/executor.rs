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
            eprintln!("Waiting for wake");
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

        if !was_notified {
            eprintln!("Awoken");
            self.cv.notify_one();
        }
    }
}
pub fn run_future<F: Future>(future: F) -> F::Output {
    pin_mut!(future);

    let notifier = Arc::new(Notifier::default());
    let waker = notifier.clone().into_waker();

    let mut cx = Context::from_waker(&waker);

    let output = loop {
        match future.as_mut().poll(&mut cx) {
            Poll::Ready(output) => break output,
            Poll::Pending => notifier.wait(),
        };
    };

    output
}
