pub struct HttpBody {
    //暂时不实现
    raw: Vec<u8>,
}

impl HttpBody {
    pub fn new() -> HttpBody {
        HttpBody {raw: vec![]}
    }
    pub fn from_bytes(bs: Vec<u8>) -> HttpBody {
        HttpBody { raw: bs }
    }
}