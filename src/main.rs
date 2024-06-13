#![allow(non_snake_case)]

use std::{fs, path::PathBuf, sync::{Arc, Mutex}};

use chrono::NaiveDateTime;
use egui::Vec2;
use egui_extras::{Column, TableBuilder};
use tiberius::{Client, Query};
use tokio::net::TcpStream;
use tokio_stream::StreamExt;
use tokio_util::compat::{Compat, TokioAsyncWriteCompatExt};

mod config;

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    let config = match config::Config::read(PathBuf::from(".\\Config.ini")) {
        Ok(c) => c,
        Err(e) => {
            println!("{e}");
            std::process::exit(0)
        }
    };

    // Tiberius configuartion:
    let mut tib_config = tiberius::Config::new();
    tib_config.host(config.server);
    tib_config.authentication(tiberius::AuthMethod::sql_server(
        config.username,
        config.password,
    ));
    tib_config.trust_cert();

    // Connect to the DB:
    let mut client_tmp = connect(tib_config.clone()).await;
    let mut tries = 0;
    while client_tmp.is_err() && tries < 3 {
        client_tmp = connect(tib_config.clone()).await;
        tries += 1;
    }

    if client_tmp.is_err() {
        println!("ER: Connection to DB failed!");
        return Ok(());
    }
    let mut client = client_tmp?;

    // USE [DB]
    let qtext = format!("USE [{}]", config.database);
    let query = Query::new(qtext);
    query.execute(&mut client).await?;

    // Start egui
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size(egui::Vec2 { x: 550.0, y: 250.0 }),
        ..Default::default()
    };

    _ = eframe::run_native(
        format!("ICT Lekérdező ({VERSION})").as_str(),
        options,
        Box::new(|_| Box::new(IctResultApp::default(client))),
    );

    Ok(())
}

async fn connect(tib_config: tiberius::Config) -> anyhow::Result<Client<Compat<TcpStream>>> {
    let tcp = TcpStream::connect(tib_config.get_addr()).await?;
    tcp.set_nodelay(true)?;
    let client = Client::connect(tib_config, tcp.compat_write()).await?;

    Ok(client)
}

fn get_pos_from_logname(log_file_name: String) -> u8 {
    println!("{}", log_file_name);
    let filename = log_file_name.split(&['/','\\']).last().unwrap();
    let pos = filename.split_once('-').unwrap();
    println!("{:?}", pos);
    pos.0.parse::<u8>().unwrap()-1
}

fn generate_serials(serial: String, position: u8, max_pos: u8) -> Vec<String> {
    let mut ret = Vec::with_capacity(max_pos as usize);
    
    let sn = serial[6..13].parse::<u32>().expect("ER: Parsing error") - position as u32; 
    for i in sn..sn+max_pos as u32 {
        let mut s = serial.clone();
        s.replace_range(6..13, &format!("{:07}", i));
        ret.push(s);
    }


    ret
}

struct Panel {
    boards: u8,
    selected_pos: u8,
    serials: Vec<String>,
    results: Vec<PanelResult>
}

impl Panel {
    fn empty() -> Self {
        Panel { boards: 0, selected_pos: 0, serials: Vec::new(), results: Vec::new()}
    }

    fn is_empty(&self) -> bool {
        self.serials.is_empty()
    }

    fn new(boards: u8) -> Self {
        Panel { 
            boards,
            selected_pos: 0,
            serials: Vec::new(),
            results: Vec::new() }
    }

    fn push(&mut self, position: u8, serial: String, station: String, result: String, date_time: NaiveDateTime) {
        if self.serials.is_empty() {
            self.serials = generate_serials(serial, position, self.boards);
            self.selected_pos = position;
            println!("Serials: {:#?}", self.serials);
        }

        let mut results = vec![BoardResult::Unknown;self.boards as usize];
        results[position as usize] = if result == "Passed" {
                BoardResult::Passed
            } else {
                BoardResult::Failed
            };

        self.results.push(PanelResult { time: date_time, station, results })
    }

    fn add_result(&mut self, i: u8, result: String) {
        let res = if result == "Passed" {
            BoardResult::Passed
        } else {
            BoardResult::Failed
        };

        for x in self.results.iter_mut() {
            if x.results[i as usize] == BoardResult::Unknown {
                x.results[i as usize] = res;
                break;
            }
        }
    }
}

struct PanelResult {
    time: NaiveDateTime,
    station: String,
    results: Vec<BoardResult>,
}

#[derive(Clone, Copy, PartialEq)]
enum BoardResult {
    Passed,
    Failed,
    Unknown
}

impl BoardResult {
    pub fn into_color(self) -> egui::Color32 {
        match self {
            BoardResult::Passed => egui::Color32::GREEN,
            BoardResult::Failed => egui::Color32::RED,
            BoardResult::Unknown => egui::Color32::YELLOW,
        }
    }
}

struct IctProducts {
    name: String, 
    DMC: String,
    boards_on_panel: u8
}

fn load_products() -> Vec<IctProducts> {
    let mut ret = Vec::new();

    if let Ok(file ) = fs::read_to_string(".\\products") {
        for line in file.lines() {
            if line.is_empty() || line.starts_with('!') {
                continue;
            }

            let parts: Vec<String> = line.split('|').map(|f| f.trim().to_owned()).collect();
            if parts.len() == 3 {
                ret.push(IctProducts { 
                    name:  parts[0].clone(),
                    DMC: parts[1].clone(), 
                    boards_on_panel: parts[2].parse().expect("Parsing error at loading products!") });
            }
            
        }
    } else {
        println!("Could not load products file!");
    }

    ret
}

struct IctResultApp {
    client: Arc<tokio::sync::Mutex<Client<Compat<TcpStream>>>>,

    products: Vec<IctProducts>,
    panel: Arc<Mutex<Panel>>,

    DMC_input: String,
}

impl IctResultApp {
    fn default(client: Client<Compat<TcpStream>>) -> Self {
        IctResultApp {
            client: Arc::new(tokio::sync::Mutex::new(client)),
            products: load_products(),
            panel: Arc::new(Mutex::new(Panel::empty())),
            DMC_input: String::new(),
        }
    }
}

impl eframe::App for IctResultApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("SNBR").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.monospace("DMC:");

                let mut text_edit = egui::TextEdit::singleline(&mut self.DMC_input).desired_width(300.0).show(ui);

                if text_edit.response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    println!("Query DMC: {}", self.DMC_input);
                    let DMC = self.DMC_input.clone();

                    let new_range = 
                    egui::text::CCursorRange::two(egui::text::CCursor::new(0), egui::text::CCursor::new(DMC.len()));
                    text_edit.response.request_focus();
                    text_edit.state.cursor.set_char_range(Some(new_range));
                    text_edit.state.store(ui.ctx(), text_edit.response.id);

                    // Identify product type
                    let mut boards_on_panel = 1;
                    println!("Product id: {}", &DMC[13..]);
                    for product in &self.products {
                        if DMC[13..].starts_with(&product.DMC) {
                            println!("Product is: {}", product.name);
                            boards_on_panel = product.boards_on_panel;
                            break;
                        }
                    }

                    self.panel = Arc::new(Mutex::new(Panel::new(boards_on_panel)));

                    // 1 - query to given DMC
                    // 2 - from Log_file_name get the board position
                    // 3 - push result to panel to the given position
                    // 4 - calculate the rest of the serials
                    // 5 - query the remaining serials

                    let panel_lock = self.panel.clone();
                    let client_lock = self.client.clone();
                    let context = ctx.clone();                    

                    tokio::spawn(async move {
                        let mut c = client_lock.lock().await;                        

                        let mut query =
                            Query::new("SELECT [Serial_NMBR],[Station],[Result],[Date_Time],[Log_File_Name] FROM [dbo].[SMT_Test] WHERE [Serial_NMBR] = @P1");
                        query.bind(&DMC);

                        println!("Query: {:?}", query);

                        let mut failed_query = true;
                        let mut position: u8 = 0;
                        if let Ok(mut result) = query.query(&mut c).await {
                            while let Some(row) = result.next().await {
                                let row = row.unwrap();
                                match row {
                                    tiberius::QueryItem::Row(x) => {
                                        // [Serial_NMBR],[Station],[Result],[Date_Time],[Log_File_Name] 
                                        let serial = x.get::<&str, usize>(0).unwrap().to_owned();
                                        let station = x.get::<&str, usize>(1).unwrap().to_owned();
                                        let result = x.get::<&str, usize>(2).unwrap().to_owned();
                                        let date_time = x.get::<NaiveDateTime, usize>(3).unwrap();
                                        let log_file_name = x.get::<&str, usize>(4).unwrap().to_owned();
                                        
                                        position = get_pos_from_logname(log_file_name);

                                        panel_lock.lock().unwrap().push(position, serial, station, result, date_time);

                                        failed_query = false;
                                    }
                                    tiberius::QueryItem::Metadata(_) => (),
                                }
                            }
                        }

                        if boards_on_panel > 1 && !failed_query {
                            for i in 0..boards_on_panel {
                                if i == position {
                                    continue;
                                }

                                let DMC = panel_lock.lock().unwrap().serials[i as usize].clone();

                                let mut query =
                                Query::new("SELECT [Result] FROM [dbo].[SMT_Test] WHERE [Serial_NMBR] = @P1");
                                query.bind(&DMC);
        
                                println!("Query #{i}: {:?}", query);

                                if let Ok(mut result) = query.query(&mut c).await {
                                    while let Some(row) = result.next().await {
                                        let row = row.unwrap();
                                        match row {
                                            tiberius::QueryItem::Row(x) => {
                                                // [Result]
                                                let result = x.get::<&str, usize>(0).unwrap().to_owned();
                                                print!("{}, ", result);
                                                panel_lock.lock().unwrap().add_result(i, result);
                                            }
                                            tiberius::QueryItem::Metadata(_) => (),
                                        }
                                    }
                                }

                                println!();
                            }
                        }

                        context.request_repaint();
                    });
                }
            });
        });
        egui::CentralPanel::default().show(ctx, |ui| {
            let panel_lock = self.panel.lock().unwrap();

            if !panel_lock.is_empty() {
                TableBuilder::new(ui)
                .striped(true)
                .column(Column::initial(40.0).resizable(true))
                .column(Column::initial(250.0).resizable(true))  // Result
                .column(Column::initial(100.0).resizable(true))  // Station
                .column(Column::initial(150.0).resizable(true)) // Time
                .header(20.0, |mut header| {
                    header.col(|ui| {
                        ui.heading("#");
                    });
                    header.col(|ui| {
                        ui.heading("Results");
                    });
                    header.col(|ui| {
                        ui.heading("Station");
                    });
                    header.col(|ui| {
                        ui.heading("Time");
                    });
                })
                .body(|mut body| {
                    for (x, result) in panel_lock.results.iter().enumerate() {
                        body.row(16.0, |mut row| {
                            row.col(|ui| {
                                ui.label(format!("{}", x+1));
                            });
                            row.col(|ui| {
                                ui.spacing_mut().interact_size = Vec2::new(0.0, 0.0);
                                ui.spacing_mut().item_spacing = Vec2::new(3.0, 3.0);

                                ui.horizontal( |ui|
                                    for (i, board) in result.results.iter().enumerate() {
                                        draw_result_box(ui, board, i == panel_lock.selected_pos as usize);
                                    }
                                );
                            });
                            row.col(|ui| {
                                ui.label(&result.station);
                            });
                            row.col(|ui| {
                                ui.label(format!( "{}", result.time.format("%Y-%m-%d %H:%M")));
                            });
                        });
                    }
                });
            }
        });
    }
}

fn draw_result_box(ui: &mut egui::Ui, result: &BoardResult, highlight: bool) -> egui::Response {
    let desired_size = egui::vec2(10.0, 10.0); 

    let (rect, response) = ui.allocate_exact_size(desired_size, egui::Sense::hover());

    let rect = if highlight {
        rect.expand(2.0)
    } else { rect };

    if ui.is_rect_visible(rect) {
        ui.painter().rect_filled(rect, 2.0, result.into_color());
    }

    response
}