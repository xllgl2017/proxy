use reqrio::{Buffer, Response};
use crate::error::ProxyResult;

pub type Request = Response;


pub struct HttpStream {
    request: Request,
    response: Response,
}

impl HttpStream {
    pub fn new() -> HttpStream {
        HttpStream {
            request: Request::new(),
            response: Response::new(),
        }
    }

    pub fn extend(&mut self, buffer: &Buffer, req: bool) -> ProxyResult<bool> {
        match req {
            true => Ok(self.request.extend(buffer)?),
            false => Ok(self.response.extend(buffer)?)
        }
    }
}