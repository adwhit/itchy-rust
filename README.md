# itchy

![Build Status](https://github.com/adwhit/itchy-rust/workflows/CI/badge.svg)
[![Crates.io Version](https://img.shields.io/crates/v/itchy.svg)](https://crates.io/crates/itchy)

ITCH parser library for Rust. Implements the NASDAQ 5.0 spec which can be found [here](http://www.nasdaqtrader.com/content/technicalsupport/specifications/dataproducts/NQTVITCHSpecification_5.0.pdf).

It is zero-allocation (thanks [nom](http://github.com/geal/nom)!)
and pretty fast, parsing around 20M messages/second on my not-fast laptop.

## Usage

Add this to your `Cargo.toml`:
```toml
[dependencies]
itchy = "0.3"
```

Simple usage example:

```rust
let stream = itchy::MessageStream::from_file("/path/to/file.itch").unwrap();
for msg in stream {
    println!("{:?}", msg.unwrap())
}
```

See the [API docs](https://docs.rs/itchy/latest/itchy/) for more information.
