#[derive(Debug, Clone)]
pub enum BObject {
    Int(i64),
    Str(Vec<u8>),
    List(Vec<BObject>),
    Dict(Vec<(String, BObject)>),
}

pub struct Parser<'a> {
    pub data: &'a [u8],
    pub pos: usize,
    pub info_range: Option<(usize, usize)>,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> u8 {
        self.data[self.pos]
    }

    fn advance(&mut self, n: usize) {
        self.pos += n;
    }

    fn parse_int(&mut self) -> BObject {
        self.advance(1);
        let start = self.pos;
        while self.peek() != b'e' {
            self.advance(1);
        }
        let end = self.pos;
        self.advance(1);
        let n = str::from_utf8(&self.data[start..end])
            .unwrap()
            .parse::<i64>()
            .unwrap();

        BObject::Int(n)
    }

    fn parse_str(&mut self) -> BObject {
        let start = self.pos;
        while self.peek() != b':' {
            self.advance(1);
        }
        let len: usize = str::from_utf8(&self.data[start..self.pos])
            .unwrap()
            .parse()
            .unwrap();

        self.advance(1);

        let bytes = self.data[self.pos..self.pos + len].to_vec();
        self.advance(len);

        BObject::Str(bytes)
    }

    fn parse_list(&mut self) -> BObject {
        self.advance(1);
        let mut out = Vec::new();
        while self.peek() != b'e' {
            out.push(self.parse_value());
        }
        self.advance(1);
        BObject::List(out)
    }

    pub fn parse_value(&mut self) -> BObject {
        match self.peek() {
            b'i' => self.parse_int(),
            b'l' => self.parse_list(),
            b'd' => self.parse_dict(),
            b'0'..=b'9' => self.parse_str(),
            _ => panic!("Invalid Bencode"),
        }
    }

    fn parse_dict(&mut self) -> BObject {
        self.advance(1);
        let mut out = Vec::new();
        while self.peek() != b'e' {
            let key = match self.parse_str() {
                BObject::Str(s) => String::from_utf8(s).unwrap(),
                _ => unreachable!(),
            };
            if key == "info" {
                let info_start = self.pos;
                let val = self.parse_value();
                let info_end = self.pos;
                self.info_range = Some((info_start, info_end));
                out.push((key, val));
            } else {
                let val = self.parse_value();
                out.push((key, val));
            }
        }
        self.advance(1);
        BObject::Dict(out)
    }

}

pub fn get_value(dict: &BObject, key: String) -> Option<BObject> {
    if let BObject::Dict(root) = dict {
        for (k, v) in root {
            if *k == key
            {
                return Some(v.clone());
            }
        }
    }
    None
}
