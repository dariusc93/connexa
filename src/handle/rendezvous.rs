use crate::handle::Connexa;

// TODO
#[allow(dead_code)]
pub struct ConnexaRendezvous<'a> {
    connexa: &'a Connexa,
}

impl<'a> ConnexaRendezvous<'a> {
    pub(crate) fn new(connexa: &'a Connexa) -> Self {
        Self { connexa }
    }
}
