use super::{Code, Frame};
use anyhow::Result;
use std::convert::TryInto;
use std::io::Write;
use crate::jenkins;

pub struct Encoder<'a, T: jenkins::Transport> {
    w: &'a mut T,
}

impl<T: jenkins::Transport> Encoder<'_, T> {
    pub fn new(writer: &mut T) -> Encoder<T> {
        Encoder { w: writer }
    }

    fn frame(&mut self, f: &Frame) -> Result<()> {
        self.w.write_frame(f)
    }

    pub fn op(&mut self, op: Code) -> Result<()> {
        self.frame(&Frame {
            op,
            data: vec![0; 0],
        })
    }

    pub fn string(&mut self, op: Code, s: &str) -> Result<()> {
        let str_bytes = s.as_bytes();
        let mut data = Vec::with_capacity(2 + str_bytes.len());
        data.write_all(&(str_bytes.len() as u16).to_be_bytes())?;
        data.write_all(str_bytes)?;
        self.frame(&Frame { op, data })
    }
}

