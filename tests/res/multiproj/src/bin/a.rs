use oasis_std::{Address, Context};

#[derive(oasis_std::Service)]
struct A;

impl A {
    pub fn new(_ctx: &Context) -> Self {
        Self
    }

    pub fn say_hello(&self, _ctx: &Context) -> String {
        b::BClient::new(Address::default());
        c::CClient::new(Address::default());
        String::new()
    }
}

fn main() {
    oasis_std::service!(A);
}
