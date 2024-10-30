use std::{
    cmp::Reverse,
    collections::{binary_heap::PeekMut, BinaryHeap, HashMap},
    future::Future,
    pin::Pin,
    sync::OnceLock,
    task::{Context, Poll, Waker},
    thread,
    time::{Duration, Instant}, // sync::WaitTimeoutResult, intrinsics::read_via_copy,
};

use crossbeam::channel::{bounded, unbounded, Receiver, RecvTimeoutError, Sender, TryRecvError};
// use futures::{SinkExt, channel::mpsc::TryRecvError};
use pin_project_lite::pin_project;
use snowflake::ProcessUniqueId;

#[derive(Debug)]
pub struct Delay {
    deadline: Instant,
    state: DelayState,
}

#[derive(Debug)]
enum Message {
    New {
        deadline: Instant,
        waker: Waker,
        notify: Sender<()>,
        id: ProcessUniqueId,
    },
    Polled {
        waker: Waker,
        id: ProcessUniqueId,
    },
}

pub fn sleep_until(deadline: Instant) -> Delay {
    Delay {
        deadline,
        state: DelayState::New,
    }
}

pub fn sleep(duration: Duration) -> Delay {
    sleep_until(Instant::now() + duration)
}

#[derive(Debug)]
enum DelayState {
    New,
    Waiting {
        signal: Receiver<()>,
        id: ProcessUniqueId,
    },
}

impl Future for Delay {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();

        match this.state {
            DelayState::New => {
                let (notify, signal) = bounded(1);
                let id = ProcessUniqueId::new();

                let message = Message::New {
                    notify,
                    waker: cx.waker().clone(),
                    deadline: this.deadline,
                    id,
                };

                this.state = DelayState::Waiting { signal, id };

                sleeper_thread_channel().send(message).unwrap();

                Poll::Pending
            }
            DelayState::Waiting { ref signal, id } => match signal.try_recv() {
                Ok(()) => Poll::Ready(()),

                Err(TryRecvError::Disconnected) => panic!("Sleeper thread droped the delay"),

                Err(TryRecvError::Empty) => {
                    let message = Message::Polled {
                        waker: cx.waker().clone(),
                        id,
                    };

                    sleeper_thread_channel().send(message).unwrap();

                    Poll::Pending
                }
            },
        }
    }
}

fn sleeper_thread_channel() -> &'static Sender<Message> {
    static CHANNEL: OnceLock<Sender<Message>> = OnceLock::new();

    CHANNEL.get_or_init(|| {
        let (sender, receiver) = unbounded();
        thread::spawn(move || {
            let mut timers: BinaryHeap<(Reverse<Instant>, ProcessUniqueId)> = BinaryHeap::new();
            let mut wakers: HashMap<ProcessUniqueId, (Waker, Sender<()>)> = HashMap::new();

            loop {
                let now = Instant::now();
                let next_event = loop {
                    match timers.peek_mut() {
                        None => break None,
                        Some(slot) => {
                            if slot.0 .0 < now {
                                let (_, id) = PeekMut::pop(slot);

                                if let Some((waker, sender)) = wakers.remove(&id) {
                                    if let Ok(()) = sender.send(()) {
                                        waker.wake();
                                    }
                                }
                            } else {
                                break Some(slot.0 .0);
                            }
                        }
                    }
                };
                let message: Message = match next_event {
                    None => receiver.recv().unwrap(),
                    Some(deadline) => match receiver.recv_deadline(deadline) {
                        Ok(message) => message,
                        Err(RecvTimeoutError::Timeout) => continue,
                        Err(RecvTimeoutError::Disconnected) => panic!("Sender was dropped"),
                    },
                };

                match message {
                    Message::New {
                        deadline,
                        waker,
                        notify,
                        id,
                    } => {
                        timers.push((Reverse(deadline), id));
                        wakers.insert(id, (waker, notify));
                    }
                    Message::Polled { waker, id } => {
                        if let Some((old_waker, _)) = wakers.get_mut(&id) {
                            *old_waker = waker;
                        }
                    }
                }
            }
        });

        sender
    })
}

pin_project! {
#[derive(Debug)]
    pub struct Timeout<F> {
        #[pin]
        future: F,

        #[pin]
        delay: Delay,
    }
}

pub fn timeout<F: Future>(futures: F, duration: Duration) -> Timeout<F> {
    Timeout {
        future: futures,
        delay: sleep(duration),
    }
}
impl<F: Future> Future for Timeout<F> {
    type Output = Option<F::Output>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();

        match this.future.poll(cx) {
            Poll::Ready(output) => Poll::Ready(Some(output)),
            Poll::Pending => match this.delay.poll(cx) {
                Poll::Ready(()) => Poll::Ready(None),
                Poll::Pending => Poll::Pending,
            },
        }
    }
}
