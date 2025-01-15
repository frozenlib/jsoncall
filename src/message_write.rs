use std::{future::Future, pin::Pin};

use super::{MessageBatch, Result};

pub trait MessageWrite {
    fn write(&mut self, batch: MessageBatch) -> impl Future<Output = Result<()>> + Sync + Send;

    fn boxed(self) -> BoxMessageWriter
    where
        Self: Sized + Send + Sync + 'static,
    {
        BoxMessageWriter(Box::new(self))
    }
}

pub struct BoxMessageWriter(Box<dyn DynMessageWrite + Send + Sync + 'static>);
impl MessageWrite for BoxMessageWriter {
    async fn write(&mut self, batch: MessageBatch) -> Result<()> {
        self.0.dyn_write(batch).await
    }
    fn boxed(self) -> BoxMessageWriter
    where
        Self: Sized + Send + Sync + 'static,
    {
        self
    }
}

trait DynMessageWrite {
    fn dyn_write<'a>(
        &'a mut self,
        batch: MessageBatch,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Sync + Send + 'a>>;
}
impl<T: MessageWrite> DynMessageWrite for T {
    fn dyn_write<'a>(
        &'a mut self,
        batch: MessageBatch,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Sync + Send + 'a>> {
        Box::pin(self.write(batch))
    }
}
