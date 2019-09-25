use oasis_std::{Address, Context};

#[derive(oasis_std::Service)]
struct B;

impl B {
    pub fn new(_ctx: &Context) -> Self {
        Self
    }

    pub fn say_hello(&self, _ctx: &Context) -> String {
        c::CClient::at(Address::default());
        String::new()
    }
}

fn main() {
    oasis_std::service!(B);
}
