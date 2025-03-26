use crate::hooks::{OnionMessage, onion_message::OnionMessageResponse};
use log::trace;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;
use std::time::SystemTime;
use tokio::sync::broadcast;
use tokio::sync::oneshot;

const MESSAGE_TIMEOUT: u64 = 30;

type PendingMessages =
    Arc<Mutex<HashMap<u64, (SystemTime, oneshot::Sender<OnionMessageResponse>)>>>;

#[derive(Clone)]
pub struct Messenger {
    tx: broadcast::Sender<OnionMessage>,
    pending_messages: PendingMessages,
}

impl Messenger {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(1024);
        Self {
            tx,
            pending_messages: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn timeout_loop(&self) {
        let mut interval = tokio::time::interval(Duration::from_secs(MESSAGE_TIMEOUT));
        trace!(
            "Timing out pending onion messages every {} seconds",
            MESSAGE_TIMEOUT
        );
        loop {
            interval.tick().await;
            self.check_timeouts();
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<OnionMessage> {
        self.tx.subscribe()
    }

    pub fn send_response(&self, id: u64, response: OnionMessageResponse) {
        if let Some((_, tx)) = self.pending_messages.lock().unwrap().remove(&id) {
            trace!("Sending response to onion message: {}", id);
            let _ = tx.send(response);
        }
    }

    pub fn received_message(
        &self,
        message: OnionMessage,
    ) -> oneshot::Receiver<OnionMessageResponse> {
        let (tx, rx) = oneshot::channel();
        trace!("Received onion message: {}", message.id());
        self.pending_messages
            .lock()
            .unwrap()
            .insert(message.id(), (SystemTime::now(), tx));
        let _ = self.tx.send(message);

        rx
    }

    fn check_timeouts(&self) {
        let mut keys_to_remove = Vec::new();
        let now = SystemTime::now();

        let mut pending_messages = self.pending_messages.lock().unwrap();
        for (id, (time, _)) in pending_messages.iter_mut() {
            if now.duration_since(*time).unwrap() > Duration::from_secs(MESSAGE_TIMEOUT) {
                keys_to_remove.push(*id);
            }
        }

        for key in keys_to_remove {
            let (_, tx) = pending_messages.remove(&key).unwrap();
            trace!("Timed out pending onion message: {}", key);
            let _ = tx.send(OnionMessageResponse::Continue);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hooks::onion_message::UnknownField;

    fn create_test_message(id: u64, data: Vec<u8>) -> OnionMessage {
        OnionMessage {
            pathsecret: Some(format!("secret_{}", id)),
            reply_blindedpath: None,
            invoice_request: None,
            invoice: None,
            invoice_error: None,
            unknown_fields: vec![UnknownField {
                number: 1,
                value: hex::encode(&data),
            }],
        }
    }

    #[tokio::test]
    async fn test_message_sending_and_receiving() {
        let messenger = Messenger::new();
        let mut rx = messenger.subscribe();

        let test_message = create_test_message(1, vec![1, 2, 3]);
        let response_rx = messenger.received_message(test_message.clone());

        let received_message = rx.recv().await.unwrap();
        assert_eq!(received_message.id(), test_message.id());

        messenger.send_response(test_message.id(), OnionMessageResponse::Continue);

        let response = response_rx.await.unwrap();
        assert_eq!(response, OnionMessageResponse::Continue);
    }

    #[tokio::test]
    async fn test_message_timeout() {
        let messenger = Messenger::new();
        let test_message = create_test_message(1, vec![1, 2, 3]);

        let (tx, rx) = oneshot::channel();
        let fake_time = SystemTime::now() - Duration::from_secs(MESSAGE_TIMEOUT + 1);
        messenger
            .pending_messages
            .lock()
            .unwrap()
            .insert(test_message.id(), (fake_time, tx));

        messenger.check_timeouts();

        let response = rx.await.unwrap();
        assert_eq!(response, OnionMessageResponse::Continue);
    }

    #[tokio::test]
    async fn test_multiple_subscribers() {
        let messenger = Messenger::new();
        let mut rx1 = messenger.subscribe();
        let mut rx2 = messenger.subscribe();

        let test_message = create_test_message(1, vec![1, 2, 3]);
        let _response_rx = messenger.received_message(test_message.clone());

        let received_message1 = rx1.recv().await.unwrap();
        let received_message2 = rx2.recv().await.unwrap();

        assert_eq!(received_message1.id(), test_message.id());
        assert_eq!(received_message2.id(), test_message.id());
    }

    #[test]
    fn test_nonexistent_message_response() {
        let messenger = Messenger::new();
        // Try to send a response for a message that doesn't exist; should not panic
        messenger.send_response(999, OnionMessageResponse::Continue);
    }
}
