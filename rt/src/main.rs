mod executor;
mod sleep;

use std::{
    future::Future, pin::Pin, task::{Context, Poll}, time::Duration
};

// use futures::future::join;

#[derive(Debug)]
struct Yield {
    yielded: bool,
}

impl Future for Yield {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        log::info!("Yielded = {} cx: {:?}", self.yielded, cx);
        if !self.yielded {
            self.yielded = true;
            cx.waker().wake_by_ref();
            Poll::Pending
        } else {
            Poll::Ready(())
        }
    }
}

async fn simple() -> i32 {
    // sleep::sleep(Duration::from_secs(5)).await;
    let p = Duration::from_secs(5);
    log::info!("going to sleep period {:?}", p);
    let sleep5 = sleep::sleep(p);

    let timeout = sleep::timeout(sleep5, Duration::from_secs(3));
    match timeout.await {
        Some(()) => println!("Success timer hit"),
        None => println!("We timed out"),
    }

    let sleep3 = sleep::sleep(Duration::from_secs(3));

    let timeout = sleep::timeout(sleep3, Duration::from_secs(5));
    match timeout.await {
        Some(()) => println!("Success timer hit"),
        None => println!("We timed out"),
    }

    // join(sleep3, sleep5).await;

    let inner = Yield { yielded: false };
    inner.await;
    10
}

fn main() -> anyhow::Result<()>{
    simple_logger::SimpleLogger::new()
        .with_level(log::LevelFilter::Trace)
        .env()
        .with_threads(true)
        .with_colors(true)
        .init()?;

    let fut = simple();

    let output = executor::run_future(fut);

    assert_eq!(output, 10);

    Ok(())
}
