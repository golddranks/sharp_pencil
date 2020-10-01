# Sharp Pencil

This is a web framework in syncronous Rust, based on [Hyper 0.10](https://docs.rs/hyper/0.10.16/hyper/index.html) and originally developed circa 2016. It's a fork of unmaintained framework [Pencil](https://github.com/fengsp/pencil/). The origin of this fork can be traced to this message: https://github.com/fengsp/pencil/issues/49

[![Build Status](https://travis-ci.org/fengsp/pencil.svg?branch=master)](https://travis-ci.org/fengsp/pencil) [![Crates.io Version](https://img.shields.io/crates/v/pencil.svg)](https://crates.io/crates/pencil/) [![Crates.io LICENSE](https://img.shields.io/crates/l/pencil.svg)](https://crates.io/crates/pencil/)

A microframework for Rust inspired by Flask.

```rust
extern crate pencil;

use sharp_pencil::{Pencil, Request, Response, PencilResult};

fn hello(_: &mut Request) -> PencilResult {
    Ok(Response::from("Hello World!"))
}

fn main() {
    let mut app = Pencil::new("/web/hello");
    app.get("/", "hello", hello);
    app.run("127.0.0.1:5000");
}
```

One simple guide: https://fengsp.github.io/blog/2016/3/introducing-pencil/

If you feel anything wrong, feedbacks or pull requests are welcome.
