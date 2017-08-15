#[macro_use]
extern crate error_chain;
#[macro_use]
extern crate nom;
extern crate flate2;

use std::io::prelude::*;
use std::fs::File;
use std::path::Path;

use flate2::read::GzDecoder;
use nom::{be_u8, be_u16, be_u32, be_u64, print, IResult, Needed};

use errors::*;

const BUFSIZE: usize = 10 * 1024;

#[allow(unused_doc_comment)]
pub mod errors {
    error_chain!{
        foreign_links {
            Io(::std::io::Error);
            Nom(::nom::Err);
        }
    }
}

pub struct MessageStream<R: Read> {
    reader: R,
    buffer: Box<[u8; BUFSIZE]>, // 10Mb buffer
    bufstart: usize,
    bufend: usize
}

impl<R: Read> MessageStream<R> {
    fn new(reader: R) -> MessageStream<R> {
        MessageStream {
            reader,
            buffer: Box::new([0; BUFSIZE]),
            bufstart: 0,
            bufend: 0
        }
    }

    fn fetch_more_bytes(&mut self) -> Result<usize> {
        println!("bufstart: {:?}, bufend: {:?}", self.bufstart, self.bufend);
        if self.bufend == BUFSIZE {
            // we need more data from the reader
            // first, copy the remnants back to the beginning of the buffer
            // (this should only be a few bytes)
            assert!(self.bufstart as usize > BUFSIZE / 2); // safety check
            assert!(BUFSIZE - self.bufstart < 50);         // extra careful check
            {
                let (mut left, right) = self.buffer.split_at_mut(self.bufstart);
                &left[..right.len()].copy_from_slice(&right[..]);
                self.bufstart = 0;
                self.bufend = right.len();
            }

        }
        Ok(self.reader.read(&mut self.buffer[self.bufend..])?)
    }
}

impl<R: Read> Iterator for MessageStream<R> {
    type Item = Result<Message>;

    fn next(&mut self) -> Option<Result<Message>> {
        use IResult::*;
        match parse_message(&self.buffer[self.bufstart..self.bufend]) {
            Done(rest, msg) => {
                self.bufstart = self.bufend - rest.len();
                return Some(Ok(msg))
            }
            Error(e) => return Some(Err(errors::Error::from(e))),
            Incomplete(_) => {
                println!("Incomplete")
                // fall through to below... necessary to appease borrow checker
            }
        }
        match self.fetch_more_bytes() {
            Ok(0) => Some(Err("Unexpected EOF".into())),
            Ok(ct) => {
                self.bufend += ct;
                self.next()
            }
            Err(e) => Some(Err(e))
        }
    }
}

pub fn parse_reader<R: Read>(reader: R) -> MessageStream<R> {
    // We will do the parsing in a streaming fashion because these
    // files are BIG and we don't want to load it all into memory
    MessageStream::new(reader)
}

pub fn parse_gzip<P: AsRef<Path>>(path: P) -> Result<MessageStream<GzDecoder<File>>> {
    let file = File::open(path)?;
    let reader = GzDecoder::new(file)?;
    Ok(parse_reader(reader))
}

pub fn parse_file<P: AsRef<Path>>(path: P) -> Result<MessageStream<File>> {
    let reader = File::open(path)?;
    Ok(parse_reader(reader))
}

#[derive(Debug, Clone, PartialEq)]
pub enum Message {
    SystemEvent {
        tracking_number: u16,
        timestamp: u64,
        event_code: EventCode
    },
    Unknown {
        length: u16,
        tag: char,
        content: Vec<u8>
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventCode {
    StartOfMessages,
    StartOfSystemHours,
    StartOfMarketHours,
    EndOfMarketHours,
    EndOfSystemHours,
    EndOfMessages
}

named!(parse_message<Message>, do_parse!(
    length: be_u16 >>
    msg: switch!(be_u8,
        b'S' => call!(parse_system_event) |
        other => map!(take!(length - 1),
                      |slice| Message::Unknown {
                          length,
                          tag: other as char,
                          content: Vec::from(slice)
                      })) >>
    (msg)
));

named!(parse_system_event<Message>, do_parse!(
    tag!([0, 0]) >>
    tracking_number: be_u16 >>
    timestamp: be_u48 >>
    event_code: alt!(
        char!('O') => { |_| EventCode::StartOfMessages } |
        char!('S') => { |_| EventCode::StartOfSystemHours } |
        char!('Q') => { |_| EventCode::StartOfMarketHours } |
        char!('M') => { |_| EventCode::EndOfMarketHours } |
        char!('E') => { |_| EventCode::EndOfSystemHours } |
        char!('C') => { |_| EventCode::EndOfMessages }
    ) >>
    (Message::SystemEvent { tracking_number, timestamp, event_code })
));

#[inline]
pub fn be_u48(i: &[u8]) -> IResult<&[u8], u64> {
    if i.len() < 6 {
        IResult::Incomplete(Needed::Size(6))
    } else {
        let res =
            ((i[0] as u64) << 40) +
            ((i[1] as u64) << 32) +
            ((i[2] as u64) << 24) +
            ((i[3] as u64) << 16) +
            ((i[4] as u64) << 8) +
            i[5] as u64;
        IResult::Done(&i[6..], res)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex_to_bytes(bytes: &[u8]) -> Vec<u8> {
        fn h2b(h: u8) -> Option<u8> {
            match h {
                v@b'0'...b'9' => Some(v - b'0'),
                v@b'a'...b'f' => Some(v - b'a' + 10),
                b' ' => None,
                _ => panic!("Invalid hex: {}", h as char)
            }
        }
        bytes.iter()
            .filter_map(|b| h2b(*b))
            .collect::<Vec<_>>()
            .chunks(2)
            .map(|slice| (slice[0] << 4) + slice[1])
            .collect()
    }

    #[test]
    fn system_event() {
        let code = b"0000 0000 286aab3b3a99 4f";
        let bytes = hex_to_bytes(&code[..]);
        let (_, out) = parse_system_event(&bytes[..]).unwrap();
        if let Message::SystemEvent{..} = out {
        } else {
            panic!("Expected SystemEvent,  found {:?}", out)
        }
    }

    #[test]
    fn full_parse() {
        let mut iter = parse_file("data/01302016.NASDAQ_ITCH50").unwrap();
        println!("{:?}", iter.next().unwrap().unwrap());
        println!("{:?}", iter.next().unwrap().unwrap());
        println!("{:?}", iter.next().unwrap().unwrap());
        println!("{:?}", iter.next().unwrap().unwrap());
    }
}
