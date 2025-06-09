use crate::handle::Connexa;

// TODO
#[allow(dead_code)]
#[derive(Copy, Clone)]
pub struct ConnexaRendezvous<'a, T> {
    connexa: &'a Connexa<T>,
}

impl<'a, T> ConnexaRendezvous<'a, T>
where
    T: Send + Sync + 'static,
{
    pub(crate) fn new(connexa: &'a Connexa<T>) -> Self {
        Self { connexa }
    }
}
