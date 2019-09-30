use oasis_std::Context;

#[derive(oasis_std::Service)]
struct D;

impl D {
    pub fn new(_ctx: &Context) -> Self {
        Self
    }

    pub fn say_hello(&self, ctx: &Context) -> String {
        format!("Hello, {}!", ctx.sender())
    }
}

fn main() {
    oasis_std::service!(D);
}
