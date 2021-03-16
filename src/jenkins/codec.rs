use super::{Code, Frame};
use anyhow::Result;
use std::convert::TryInto;
use std::io::Write;

pub struct Encoder {
    buf: Vec<u8>,
}

impl Encoder {
    pub fn new() -> Encoder {
        Encoder {
            buf: Vec::with_capacity(1024),
        }
    }

    fn frame(&mut self, f: &Frame) -> Result<()> {
        self.buf.write(&(f.data.len() as u32).to_be_bytes())?;
        self.buf.write(&(f.op as u8).to_be_bytes())?;
        self.buf.write(&f.data)?;
        Ok(())
    }

    pub fn op(&mut self, op: Code) -> Result<()> {
        self.frame(&Frame {
            op: op,
            data: Vec::new(),
        })
    }

    pub fn string<'a>(&mut self, op: Code, s: &'a str) -> Result<()> {
        let str_bytes = s.as_bytes();
        let mut data = Vec::with_capacity(2 + str_bytes.len());
        data.write(&(str_bytes.len() as u16).to_be_bytes())?;
        data.write(str_bytes)?;
        self.frame(&Frame { op: op, data: data })
    }

    pub fn buffer(&self) -> Vec<u8> {
        self.buf.clone()
    }
}

pub struct Decoder<'a, T: std::io::Read> {
    r: &'a mut T,
}

impl<T: std::io::Read> Decoder<'_, T> {
    pub fn new(reader: &mut T) -> Decoder<T> {
        Decoder { r: reader }
    }

    pub fn skip_initial_zero(&mut self) -> Result<()> {
        let mut buf: [u8; 1] = [42; 1];
        self.r.read_exact(&mut buf)?;
        assert_eq!(buf[0], 0);
        Ok(())
    }

    pub fn frame(&mut self) -> Result<Frame> {
        let mut buf = [0; 4];
        self.r.read_exact(&mut buf)?;
        let len = u32::from_be_bytes(buf) as usize;

        self.r.read_exact(&mut buf[0..1])?;
        let op = buf[0].try_into()?;

        let mut data = Vec::with_capacity(len);
        data.resize(len, 0);
        self.r.read_exact(&mut data)?;
        Ok(Frame { op: op, data: data })
    }
}
