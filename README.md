# itchy

[![Build Status](https://travis-ci.org/adwhit/itchy-rust.svg?branch=master)](https://travis-ci.org/adwht/itchy-rust)

ITCH parser library for Rust. Implements the NASDAQ 5.0 spec which can be found [here](http://www.nasdaqtrader.com/content/technicalsupport/specifications/dataproducts/NQTVITCHSpecification_5.0.pdf).

It is based on [nom](http://github.com/geal/nom) and despite not having
been optimised, it is pretty fast, benching ~1.5M messages/second
on my laptop (Intel Core m3-6Y30)

## Usage

Add this to your `Cargo.toml`:
```toml
[dependencies]
itchy = "0.1"
```
and this to your crate root:

```rust
extern crate itchy;
```

Simple example:

```rust
let stream = itchy::MessageStream::from_file("/path/to/file.itch").unwrap();
for msg in stream {
    println!("{:?}", msg.unwrap())
}
```

See the [API docs](http://fake) for more information.
