// this code is bad. i am very sleepy 

use askama::Template;
use hex_color::HexColor; // i am lazy
use lazy_static::lazy_static;
use light_and_shadow::{ColorDistance, Palette};
use std::env;

#[derive(Template)]
#[template(path = "message.svg", escape = "html")]
struct SvgTemplate {
    foreground: String,
    background: String,
}

static DISCORD_DARK_MODE: [u8; 3] = [54, 57, 64];
static DISCORD_LIGHT_MODE: [u8; 3] = [255, 255, 255];
static USAGE: &'static str = include_str!("../usage.md");

lazy_static! {
    static ref DEFAULT_PALETTE: Palette =
        Palette::build(vec![DISCORD_DARK_MODE, DISCORD_LIGHT_MODE], 3.4);
    static ref USVG_OPTIONS: usvg::Options = {
        let mut opt = usvg::Options::default();

        opt.resources_dir = std::fs::canonicalize(env::current_dir().unwrap())
            .ok()
            .and_then(|p| p.parent().map(|p| p.to_path_buf()));
        opt.fontdb.load_system_fonts();
        opt.fontdb.load_font_file("./opensans.otf");
        opt.fontdb.set_sans_serif_family("Open Sans");

        opt
    };
}

fn render_template(fg: [u8; 3], bg: [u8; 3]) -> Vec<u8> {
    let svg_data = SvgTemplate {
        foreground: format!("rgb({},{},{})", fg[0], fg[1], fg[2]),
        background: format!("rgb({},{},{})", bg[0], bg[1], bg[2]),
    }
    .render()
    .unwrap();

    let rtree = usvg::Tree::from_data(&svg_data.as_bytes(), &USVG_OPTIONS.to_ref()).unwrap();

    let pixmap_size = rtree.svg_node().size.to_screen_size();
    let mut pixmap = tiny_skia::Pixmap::new(pixmap_size.width(), pixmap_size.height()).unwrap();
    resvg::render(
        &rtree,
        usvg::FitTo::Original,
        tiny_skia::Transform::default(),
        pixmap.as_mut(),
    )
    .unwrap();

    pixmap.encode_png().unwrap()
}

use serenity::{
    async_trait,
    model::{channel::Message, gateway::Ready},
    prelude::*,
};

macro_rules! send_msg {
    ($http:expr, $channel:expr, $msg:expr) => {
        if let Err(why) = $channel.say($http, $msg).await {
            println!("Error sending message: {:?}", why);
        }
    };
}

struct Handler;

#[async_trait]
impl EventHandler for Handler {
    async fn message(&self, ctx: Context, msg: Message) -> () {
        if msg.content.starts_with("~asakusa") {
            let args = msg.content.split_whitespace().collect::<Vec<&str>>();

            match args[1] {
                "help" => {
                    send_msg!(&ctx.http, msg.channel_id, USAGE);
                }
                "match" => {
                    if args.len() < 3 {
                        send_msg!(&ctx.http, msg.channel_id, "please specify a hex code!");
                        return;
                    }

                    match args[2].parse::<HexColor>() {
                        Ok(color) => {
                            let (closest, _) = DEFAULT_PALETTE
                                .find_closest([color.r, color.g, color.b], ColorDistance::CIE94);

                            let light = render_template(closest, DISCORD_DARK_MODE);
                            let dark = render_template(closest, DISCORD_LIGHT_MODE);

                            if let Err(why) = msg
                                .channel_id
                                .send_files(
                                    &ctx.http,
                                    [(&light[..], "light_mode.png"), (&dark[..], "dark_mode.png")],
                                    |m| {
                                        m.content(format!(
                                            "closest color found: {}",
                                            HexColor::new(closest[0], closest[1], closest[2])
                                        ))
                                    },
                                )
                                .await
                            {
                                println!("Error sending message: {:?}", why);
                            }
                        }
                        Err(_) => {
                            send_msg!(&ctx.http, msg.channel_id, "invalid hex code!")
                        }
                    }
                }
                _ => {
                    send_msg!(&ctx.http, msg.channel_id, "invalid command");
                    send_msg!(&ctx.http, msg.channel_id, USAGE);
                }
            }
        }
    }

    async fn ready(&self, _: Context, ready: Ready) {
        println!("{} is connected!", ready.user.name);
    }
}

#[tokio::main]
async fn main() {
    let token = env::var("DISCORD_TOKEN").expect("Expected a token in the environment");

    let mut client = Client::builder(&token)
        .event_handler(Handler)
        .await
        .expect("Err creating client");

    if let Err(why) = client.start().await {
        println!("Client error: {:?}", why);
    }
}
