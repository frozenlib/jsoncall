use std::{future::Future, pin::Pin};

use super::{MessageBatch, Result};

pub trait MessageRead {
    fn read(&mut self) -> impl Future<Output = Result<Option<MessageBatch>>> + Send + Sync;
    fn boxed(self) -> BoxMessageReader
    where
        Self: Sized + Send + Sync + 'static,
    {
        BoxMessageReader(Box::new(self))
    }
}
trait DynMessageRead {
    fn dyn_read<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<Option<MessageBatch>>> + Send + Sync + 'a>>;
}
impl<T: MessageRead> DynMessageRead for T {
    fn dyn_read<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<Option<MessageBatch>>> + Send + Sync + 'a>> {
        Box::pin(self.read())
    }
}
pub struct BoxMessageReader(Box<dyn DynMessageRead + Send + Sync + 'static>);
impl MessageRead for BoxMessageReader {
    fn read(&mut self) -> impl Future<Output = Result<Option<MessageBatch>>> + Send + Sync {
        self.0.dyn_read()
    }
    fn boxed(self) -> BoxMessageReader
    where
        Self: Sized + Send + Sync + 'static,
    {
        self
    }
}
