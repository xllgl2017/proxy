use crate::data::ui::ProxyTab;
use crate::data::FilterMode;
use eframe::emath::Align;
use eframe::epaint::text::TextWrapMode;
use eframe::{App, Frame};
use egui::{include_image, Button, CentralPanel, Color32, Context, FontData, Id, Label, Layout, ScrollArea, Sense, Ui, UiBuilder, Visuals, Widget};
use std::error::Error;
use reqrio::Response;
use crate::data::http::Request;

pub struct HttpMessage {
    request: Request,
    response: Response,
}

pub struct ProxyView {
    data: Vec<HttpMessage>,
    current_item: Option<usize>,
    working: bool,
    filter_mode: FilterMode,
    view_tab: ProxyTab,
}

impl ProxyView {
    pub fn new(ctx: &eframe::CreationContext) -> Result<Box<dyn App>, Box<dyn Error + Send + Sync + 'static>> {
        //修改默认字体，确保支持中文
        let mut fonts = egui::FontDefinitions::default();
        let font_bytes = include_bytes!("../../res/font/simfang.ttf");
        let font_data = FontData::from_static(font_bytes);
        fonts.font_data.insert("my_font".to_owned(), std::sync::Arc::new(font_data));
        fonts.families.entry(egui::FontFamily::Proportional).or_default().insert(0, "my_font".to_owned());
        fonts.families.entry(egui::FontFamily::Monospace).or_default().push("my_font".to_owned());
        ctx.egui_ctx.set_fonts(fonts);
        //修改为亮模式
        ctx.egui_ctx.set_visuals(Visuals::light());
        //安装图片加载器
        egui_extras::install_image_loaders(&ctx.egui_ctx);
        Ok(Box::new(ProxyView {
            data: vec![],
            current_item: None,
            working: false,
            filter_mode: FilterMode::None,
            view_tab: ProxyTab::Header,
        }))
    }

    fn show_root_top(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            // ui.painter().rect_filled(ui.max_rect(), 0.0, Color32::BLUE);
            // ui.set_height(50.0);
            let img = if self.working { include_image!("../../res/imgs/stop.png") } else { include_image!("../../res/imgs/start.png") };
            let btn = Button::image_and_text(img, if self.working { "停止" } else { "启动" });
            ui.add(btn).clicked().then(|| self.working = !self.working);
            let btn = Button::image_and_text(include_image!("../../res/imgs/save.png"), "保存");
            ui.add(btn).clicked().then(|| {});
            let btn = Button::image_and_text(include_image!("../../res/imgs/export.png"), "导出");
            ui.add(btn).clicked().then(|| {});
            for mode in FilterMode::modes() {
                ui.selectable_label(self.filter_mode == mode, mode.to_string()).clicked().then(|| self.filter_mode = mode);
            }
        });
    }

    fn show_item(&mut self, index: usize, ui: &mut Ui) {
        let item_rect = ui.max_rect();
        let mut item_layout_rect = ui.max_rect();
        item_layout_rect.min.x = item_layout_rect.min.x + 2.0;
        item_layout_rect.min.y = item_layout_rect.min.y + 2.0;
        item_layout_rect.max.x = item_layout_rect.max.x - 2.0;
        item_layout_rect.max.y = item_layout_rect.max.y - 2.0;
        let builder = UiBuilder::new().max_rect(item_layout_rect);
        ui.allocate_new_ui(builder, |ui| {
            let datum = &self.data[index];
            ui.vertical(|ui| {
                //这里的样式我们后面再更换
                if let Some(current_item) = self.current_item {
                    if current_item == index {
                        ui.painter().rect_filled(item_rect, 0.2, Color32::LIGHT_BLUE);
                    }
                }
                let resp = ui.interact(item_rect, Id::from(format!("item_{}", index)), Sense::click_and_drag());
                if resp.hovered() {
                    ui.painter().rect_filled(item_rect, 0.2, Color32::LIGHT_YELLOW);
                }
                if resp.clicked() {
                    self.current_item = Some(index);
                }
                //保证不自动换行
                let url = Label::new("https://docs.rs/eframe/latest/eframe/").wrap_mode(TextWrapMode::Extend).truncate();
                ui.add(url);
                ui.horizontal(|ui| {
                    ui.label(index.to_string());
                    ui.label(200.to_string());
                    ui.label("文档");
                    ui.label("08:00");
                    ui.label("1.6 Kb");
                });
            });
        });
    }

    fn show_root_middle_left(&mut self, ui: &mut Ui) {
        /*
          -------------------------------------
          |  URL                              |
          -------------------------------------
          | 编 号 | 状态码 | 类型 | 时间 | 总大小 |
          ------------------------------------
         */
        ui.vertical(|ui| {
            ui.set_width(400.0);
            let area = ScrollArea::vertical().auto_shrink([false; 2]).stick_to_bottom(true);
            area.show_rows(ui, 50.0, self.data.len(), |ui, rows| {
                for row in rows { self.show_item(row, ui); }
            });
        });
    }

    fn show_header_item(&self, ui: &mut Ui, key: impl AsRef<str>, value: impl AsRef<str>) {
        let layout = Layout::right_to_left(Align::Min).with_main_align(Align::Min);
        ui.with_layout(layout, |ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            let layout = Layout::left_to_right(Align::Min);
            let mut max_rect = ui.max_rect();
            max_rect.min.x = max_rect.min.x + 100.0;
            let builder = UiBuilder::new().max_rect(max_rect).layout(layout);
            let value_resp = ui.allocate_new_ui(builder, |ui| {
                let label = Label::new(value.as_ref()).wrap_mode(TextWrapMode::Wrap);
                ui.add(label);
            });
            ui.vertical(|ui| {
                let value_height = value_resp.response.rect.height();
                if value_height > 15.0 { ui.add_space((value_height - 15.0) / 2.0); }
                let label = Label::new(key.as_ref()).wrap_mode(TextWrapMode::Wrap);
                ui.add(label);
            })
        });
    }

    fn show_headers(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.set_height(30.0);
            let rect = ui.max_rect();
            ui.painter().rect_filled(rect, 0.0, Color32::LIGHT_BLUE);
            ui.label("总揽");
        });
        self.show_header_item(ui, "请求URL", r#"data:image/svg+xml,<svg width="18" height="18" viewBox="0 0 12 12" %09enable-background="new 0 0 12 12" xmlns="http://www.w3.org/2000/svg" fill="none">%09<circle r="5.25" cx="6" cy="6" stroke-width="1.25" stroke="black"/>%09<text x="6" y="7" style="font:8px sans-serif;font-weight:1000" text-anchor="middle" %09%09dominant-baseline="middle" fill="black">?%3C/text%3E%3C/svg%3Edfjkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkk"#);
        self.show_header_item(ui, "请求方法", "POST");
        self.show_header_item(ui, "状态码", "200");
        self.show_header_item(ui, "目标地址", "127.0.0.1:443");
        ui.horizontal(|ui| {
            ui.set_height(30.0);
            let rect = ui.max_rect();
            ui.painter().rect_filled(rect, 0.0, Color32::LIGHT_BLUE);
            ui.label("请求标头");
        });
        let datum = &self.data[self.current_item.unwrap_or(0)];
        // for (key, value) in datum.request().header().keys() {
        //     self.show_header_item(ui, key, value);
        // }
        ui.horizontal(|ui| {
            ui.set_height(30.0);
            let rect = ui.max_rect();
            ui.painter().rect_filled(rect, 0.0, Color32::LIGHT_BLUE);
            ui.label("响应标头");
        });
        // for (key, value) in datum.response().header().keys() {
        //     self.show_header_item(ui, key, value);
        // }
    }
    fn show_root_middle_right(&mut self, ui: &mut Ui) {
        /*
           |标头|负载|预览|Cookie|原始请求|原始响应|
           |---------------------------------|
           |          [对应页面]               |
           -----------------------------------
         */
        let app_height = ui.max_rect().height();
        // let app_width = ui.max_rect().width();
        ui.vertical(|ui| {
            ui.set_height(app_height);
            // ui.set_width(app_width);
            ui.horizontal(|ui| {
                for tab in ProxyTab::tabs() {
                    ui.selectable_label(self.view_tab == tab, tab.to_string()).clicked().then(|| self.view_tab = tab);
                }
            });
            let area = ScrollArea::vertical().auto_shrink([false; 2]).id_salt("root_middle_right_scroll")
                .max_height(app_height);
            area.show(ui, |ui| {
                match self.view_tab {
                    ProxyTab::Header => { ui.vertical(|ui| self.show_headers(ui)); }
                    ProxyTab::PreView => {}
                    ProxyTab::Param => {}
                    ProxyTab::Cookie => {}
                    ProxyTab::ReqRaw => {}
                    ProxyTab::RespRaw => {}
                }
            });
        });
    }
}

impl App for ProxyView {
    fn update(&mut self, ctx: &Context, _frame: &mut Frame) {
        CentralPanel::default().show(ctx, |ui| {
            self.show_root_top(ui);
            let app_height = ui.max_rect().height();
            ui.horizontal(|ui| {
                ui.set_height(app_height);
                self.show_root_middle_left(ui);
                if self.current_item.is_none() { return; }
                self.show_root_middle_right(ui);
            })
        });
    }
}