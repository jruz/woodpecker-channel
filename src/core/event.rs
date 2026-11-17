use crate::core::notification::Notification;
use crate::libs::woodpecker::client;
use crate::libs::woodpecker::types::Pipeline;

#[derive(Debug)]
pub enum Event {
    InputClosed,
    InputFailed(std::io::Error),
    Line(String),
    Notification(Notification),
    Polled {
        key: String,
        result: Result<Vec<Pipeline>, client::Error>,
    },
}

pub type Sender = tokio::sync::mpsc::UnboundedSender<Event>;
