# itchy

ITCH parser in Rust 

Based off the NASDAQ 5.0 spec which can be found [here](http://www.nasdaqtrader.com/content/technicalsupport/specifications/dataproducts/NQTVITCHSpecification_5.0.pdf).

It is based on [nom](http://github.com/geal/nom) and despite not having been optimised, it
is pretty fast, benching 1.5M messages/second on my laptop (Intel Core m3-6Y30)

Todo:
* Travis CI
* Add final message type: 'B' (Broken Trade)
* Finish `parse_stock_directory` enums
