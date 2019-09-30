use oasis_std::{Address, Context};

#[derive(oasis_std::Service)]
struct A;

impl A {
    pub fn new(_ctx: &Context) -> Self {
        Self
    }

    pub fn say_hello(&self, _ctx: &Context) -> String {
        b::BClient::at(Address::default());
        c::CClient::at(Address::default());
        String::new()
    }
}

fn main() {
    oasis_std::service!(A);
}
