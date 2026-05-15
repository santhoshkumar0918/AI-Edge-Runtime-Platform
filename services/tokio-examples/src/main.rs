use tokio::{sync::mpsc, time::{sleep, Duration}};

#[tokio::main]
async fn main() {
    println!("tokio example: starting");

    let (tx, mut rx) = mpsc::channel::<String>(8);

    // spawn a producer task that sends messages
    let producer = tokio::spawn(async move {
        for i in 0..3 {
            let msg = format!("message-{}", i);
            if tx.send(msg).await.is_err() {
                // receiver dropped
                break;
            }
            sleep(Duration::from_millis(200)).await;
        }
        println!("producer done");
    });

    // spawn a background worker
    let background = tokio::spawn(async {
        for i in 0..5 {
            println!("background tick {}", i);
            sleep(Duration::from_millis(100)).await;
        }
    });

    // consume messages from the channel
    while let Some(msg) = rx.recv().await {
        println!("received: {}", msg);
    }

    // wait for tasks
    let _ = producer.await;
    let _ = background.await;

    println!("tokio example: finished");
}
