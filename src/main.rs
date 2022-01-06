// this code is bad. i am very sleepy

use askama::Template;
use clap::{App, Arg};
use hex_color::HexColor; // i am lazy
use lazy_static::lazy_static;
use light_and_shadow::{ColorDistance, Palette};
use serenity::{
    async_trait,
    client::bridge::gateway::GatewayIntents,
    model::{channel::Message, gateway::Ready, prelude::*},
    prelude::*,
    utils::Colour,
};
use std::env;
use std::time::Duration;

#[derive(Template)]
#[template(path = "message.svg", escape = "html")]
struct SvgTemplate {
    foreground: String,
    background: String,
}

static DISCORD_DARK_MODE: [u8; 3] = [54, 57, 64];
static DISCORD_LIGHT_MODE: [u8; 3] = [255, 255, 255];

lazy_static! {
    static ref DEFAULT_PALETTE: Palette =
        Palette::build(vec![DISCORD_DARK_MODE, DISCORD_LIGHT_MODE], 3.4);
    static ref USVG_OPTIONS: usvg::Options = {
        let mut opt = usvg::Options::default();

        opt.resources_dir = std::fs::canonicalize(env::current_dir().unwrap())
            .ok()
            .and_then(|p| p.parent().map(|p| p.to_path_buf()));
        opt.fontdb.load_system_fonts();
        opt.fontdb.load_font_file("./opensans.otf").unwrap();
        opt.fontdb.set_sans_serif_family("Open Sans");

        opt
    };
    static ref APP: clap::App<'static> = {
        App::new("asakusa")
        .about("a color accessibility bot")
        .author("allie signet <allie@sibr.dev>")
        .version("0.0.1")
        .subcommand(
            App::new("match")
                .about("match color to closest one with 3.4:1 or better contrast")
                .arg(
                    Arg::new("multiple")
                        .short('m')
                        .long("multiple")
                        .help("gets multiple closest colors")
                        .takes_value(true)
                        .validator(|v| match v.parse::<usize>() {
                            Ok(v) => {
                                if v < 65 {
                                    Ok(())
                                } else {
                                    Err("maximum of colors is 64".to_string())
                                }
                            }
                            Err(_) => Err("invalid number".to_string()),
                        }),
                )
                .arg(Arg::new("color").required(true).takes_value(true)),
        )
        .subcommand(
            App::new("fix")
                .about("finds the closest >3.4:1 contrast color for a role, and optionally edits it.")
                .arg(Arg::new("role").required(true).takes_value(true))
        )
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
            let matches = match APP.clone().try_get_matches_from(args) {
                Ok(v) => v,
                Err(e) => {
                    send_msg!(&ctx.http, msg.channel_id, format!("```{}```", e));
                    return;
                }
            };

            if let Some(matches) = matches.subcommand_matches("match") {
                let color = matches
                    .value_of("color")
                    .unwrap()
                    .parse::<HexColor>()
                    .unwrap();
                let results = match matches.value_of("multiple") {
                    Some(v) => {
                        let how_many = v.parse::<usize>().unwrap();
                        DEFAULT_PALETTE
                            .find_closest_n(
                                [color.r, color.g, color.b],
                                ColorDistance::CIE94,
                                how_many,
                            )
                            .into_iter()
                            .map(|(_, b)| b)
                            .collect()
                    }
                    None => {
                        vec![
                            DEFAULT_PALETTE
                                .find_closest([color.r, color.g, color.b], ColorDistance::CIE94)
                                .0,
                        ]
                    }
                };

                for (i, color) in results.into_iter().enumerate() {
                    let light = render_template(color, DISCORD_DARK_MODE);
                    let dark = render_template(color, DISCORD_LIGHT_MODE);
                    if let Err(why) = msg
                        .channel_id
                        .send_files(
                            &ctx.http,
                            [(&light[..], "light_mode.png"), (&dark[..], "dark_mode.png")],
                            |m| {
                                m.content(format!(
                                    "#{} color found: {}",
                                    i + 1,
                                    HexColor::new(color[0], color[1], color[2])
                                ))
                            },
                        )
                        .await
                    {
                        println!("Error sending message: {:?}", why);
                    }
                }
            } else if let Some(_) = matches.subcommand_matches("fix") {
                let guild = Guild::get(&ctx.http,msg.guild_id.unwrap()).await.unwrap();
                let perms = guild.member_permissions(&ctx.http, &msg.author).await.unwrap();

                if perms.administrator() || perms.manage_roles() 
                {
                    let guild_roles = msg.guild_id.unwrap().roles(&ctx.http).await.unwrap();

                    for role in msg.mention_roles {
                        let current_color = guild_roles[&role].colour;
                        let (color, _) = DEFAULT_PALETTE.find_closest(
                            [current_color.r(), current_color.g(), current_color.b()],
                            ColorDistance::CIE94,
                        );
                        let light = render_template(color, DISCORD_DARK_MODE);
                        let dark = render_template(color, DISCORD_LIGHT_MODE);
                        msg.channel_id
                            .send_files(
                                &ctx.http,
                                [(&light[..], "light_mode.png"), (&dark[..], "dark_mode.png")],
                                |m| {
                                    m.content(format!(
                                        "closest color found for role {}: {}",
                                        guild_roles[&role],
                                        HexColor::new(color[0], color[1], color[2])
                                    ))
                                },
                            )
                            .await
                            .unwrap();
                        
                        send_msg!(&ctx.http, msg.channel_id, "edit role? (y/n)");

                        let reply = msg
                            .channel_id
                            .await_reply(&ctx)
                            .author_id(*msg.author.id.as_u64())
                            .filter(|m| ["y", "yes", "n", "no"].contains(&m.content.trim()))
                            .timeout(Duration::from_secs(300));

                        if let Some(r) = reply.await {
                            if ["y", "yes"].contains(&r.content.trim()) {
                                guild_roles[&role].edit(&ctx.http, |new_role| {
                                    new_role.colour(
                                        Colour::from_rgb(color[0], color[1], color[2]).0 as u64,
                                    )
                                }).await.unwrap();
                                send_msg!(&ctx.http, msg.channel_id, format!("edited! here: {}",guild_roles[&role]));
                            } else {
                                send_msg!(&ctx.http, msg.channel_id, "alright, continuing!");
                                continue;
                            }
                        } else {
                            return;
                        }
                    }
                } else {
                    send_msg!(
                        &ctx.http,
                        msg.channel_id,
                        "you don't have the permissions to use this command!"
                    );
                }
            };
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
        .intents(
            GatewayIntents::GUILD_MEMBERS | GatewayIntents::GUILDS | GatewayIntents::GUILD_MESSAGES,
        )
        .await
        .expect("Err creating client");

    if let Err(why) = client.start().await {
        println!("Client error: {:?}", why);
    }
}
