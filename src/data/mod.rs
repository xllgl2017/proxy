pub mod http;
pub mod ui;

use std::fmt::{Display, Formatter, Write};


#[derive(Eq, PartialEq)]
pub enum FilterMode {
    None,
    XHR,
    Document,
    Css,
    Js,
    Font,
    Image,
    Media,
    Ws,
}

impl FilterMode {
    pub fn modes() -> [FilterMode; 9] {
        [FilterMode::None, FilterMode::XHR, FilterMode::Document, FilterMode::Css, FilterMode::Js,
            FilterMode::Font, FilterMode::Image, FilterMode::Media, FilterMode::Ws]
    }
}

impl Display for FilterMode {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            FilterMode::None => f.write_str("无"),
            FilterMode::XHR => f.write_str("XHR"),
            FilterMode::Document => f.write_str("文档"),
            FilterMode::Css => f.write_str("Css"),
            FilterMode::Js => f.write_str("Js"),
            FilterMode::Font => f.write_str("字体"),
            FilterMode::Image => f.write_str("图片"),
            FilterMode::Media => f.write_str("媒体"),
            FilterMode::Ws => f.write_str("套接字")
        }
    }
}