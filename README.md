# itchy

ITCH parser in Rust 

Implements the NASDAQ 5.0 spec which can be found [here](http://www.nasdaqtrader.com/content/technicalsupport/specifications/dataproducts/NQTVITCHSpecification_5.0.pdf).

It is based on [nom](http://github.com/geal/nom) and despite not having been optimised, it
is pretty fast, benching 1.5M messages/second on my laptop (Intel Core m3-6Y30)

Typical usage:

```rust
extern crate itchy;

let stream = itch::MessageStream::from_file("/path/to/file.itch").unwrap();
for msg in stream {
    println!("{:?}", msg.unwrap())
}
```

See the [API docs](http://fake) for more information.

Todo:
* Travis CI
* Interpret Price4/Price8
* Publish on crates.io
