use oasis_std::Context;

#[derive(oasis_std::Service)]
struct C;

impl C {
    pub fn new(_ctx: &Context) -> Self {
        Self
    }

    pub fn say_hello(&self, ctx: &Context) -> String {
        format!("Hello, {}!", ctx.sender())
    }
}

fn main() {
    oasis_std::service!(C);
}

