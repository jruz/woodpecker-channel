use crate::core::notification::Notification;

#[derive(Debug)]
pub enum Event {
    InputClosed,
    InputFailed(std::io::Error),
    Line(String),
    Notification(Notification),
}

pub type Sender = tokio::sync::mpsc::UnboundedSender<Event>;
