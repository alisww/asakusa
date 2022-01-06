use poise::serenity_prelude as serenity;
use serenity::Colour;

type Data = ();
type Error = Box<dyn std::error::Error + Send + Sync>;
type Context<'a> = poise::Context<'a, Data, Error>;

use askama::Template;
use hex_color::HexColor; // i am lazy
use lazy_static::lazy_static;
use light_and_shadow::{contrast_rgb, ColorDistance, Palette};
use std::collections::HashSet;
use std::env;
use std::time::Duration;

#[derive(Template)]
#[template(path = "message.svg", escape = "html")]
struct SvgTemplate {
    foreground: String,
    background: String,
    font: String,
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
        opt.fontdb
            .load_font_file("./open-sans/opensans-medium.otf")
            .unwrap();
        opt.fontdb
            .load_font_file("./open-sans/opensans.otf")
            .unwrap();

        opt
    };
}

fn render_template(fg: [u8; 3], bg: [u8; 3], bold: bool) -> Vec<u8> {
    let svg_data = SvgTemplate {
        foreground: format!("rgb({},{},{})", fg[0], fg[1], fg[2]),
        background: format!("rgb({},{},{})", bg[0], bg[1], bg[2]),
        font: if bold {
            "'Open Sans Medium'".to_owned()
        } else {
            "'Open Sans'".to_owned()
        },
    }
    .render()
    .unwrap();

    let rtree = usvg::Tree::from_data(svg_data.as_bytes(), &USVG_OPTIONS.to_ref()).unwrap();

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

/// make a role's color more accessible
#[poise::command(slash_command, required_permissions = "MANAGE_ROLES")]
async fn fix(
    ctx: Context<'_>,
    #[description = "role to modify"] role: serenity::Role,
    #[description = "display preview with username font-weight"]
    #[flag]
    bold: bool,
) -> Result<(), Error> {
    let self_id = ctx.discord().cache.current_user_id();
    let self_role = ctx
        .guild_id()
        .unwrap()
        .roles(&ctx.discord())
        .await?
        .into_values()
        .find(|r| r.tags.bot_id.is_some() && r.tags.bot_id.unwrap() == self_id)
        .unwrap();

    if self_role.position < role.position {
        ctx.say(format!(
            "that role is above me on the hierarchy; put the {} role above it to enable editing.",
            self_role
        ))
        .await?;
        return Ok(());
    }

    let current_color = role.colour;
    let (color, _) = DEFAULT_PALETTE.find_closest(
        [current_color.r(), current_color.g(), current_color.b()],
        ColorDistance::CIE94,
    );

    let dark = render_template(color, DISCORD_DARK_MODE, bold);
    let light = render_template(color, DISCORD_LIGHT_MODE, bold);

    let contrast_light = contrast_rgb(color, DISCORD_LIGHT_MODE);
    let contrast_dark = contrast_rgb(color, DISCORD_DARK_MODE);

    let handle = ctx
        .send(|m| {
            m.content(format!(
                "closest color found for role {}: {}\ncontrast on dark mode: {:.2}\ncontrast on light mode: {:.2}",
                role,
                HexColor::new(color[0], color[1], color[2]),
                contrast_dark,
                contrast_light
            ))
        })
        .await?
        .unwrap();
    if let poise::ReplyHandle::Application { http, interaction } = handle {
        let reply = interaction
            .create_followup_message(http, |m| {
                m.files([(&light[..], "light_mode.png"), (&dark[..], "dark_mode.png")])
                    .components(|c| {
                        c.create_action_row(|r| {
                            r.create_button(|b| {
                                b.style(serenity::ButtonStyle::Success)
                                    .label("edit role to match")
                                    .custom_id("yes")
                            })
                            .create_button(|b| {
                                b.style(serenity::ButtonStyle::Danger)
                                    .label("keep role as is")
                                    .custom_id("no")
                            })
                        })
                    })
            })
            .await?;

        if let Some(res) = reply
            .await_component_interaction(&ctx.discord())
            .author_id(ctx.author().id)
            .message_id(reply.id)
            .timeout(Duration::from_secs(60 * 5))
            .await
        {
            if res.data.custom_id == "yes" {
                role.edit(&http, |new_role| {
                    new_role.colour(Colour::from_rgb(color[0], color[1], color[2]).0 as u64)
                })
                .await?;

                res.create_interaction_response(&ctx.discord(), |fin| {
                    fin.interaction_response_data(|d| d.content(format!("role {} edited!", role)))
                })
                .await?;
            } else {
                res.create_interaction_response(&ctx.discord(), |fin| {
                    fin.interaction_response_data(|d| {
                        d.content(format!("okay, keeping role {} as is!", role))
                    })
                })
                .await?;
            }
        }
    }

    Ok(())
}

/// find the closest color with contrast of 3.4:1 on both discord backgrounds.
#[poise::command(slash_command, rename = "match")]
async fn match_color(
    ctx: Context<'_>,
    #[description = "color to match"] current_color: HexColor,
    #[description = "display preview with username font-weight"]
    #[flag]
    bold: bool,
) -> Result<(), Error> {
    let current_color = [current_color.r, current_color.g, current_color.b];
    let (color, _) = DEFAULT_PALETTE.find_closest(current_color, ColorDistance::CIE94);

    let cur_dark = render_template(current_color, DISCORD_DARK_MODE, bold);
    let cur_light = render_template(current_color, DISCORD_LIGHT_MODE, bold);

    let cur_contrast_dark = contrast_rgb(current_color, DISCORD_DARK_MODE);
    let cur_contrast_light = contrast_rgb(current_color, DISCORD_LIGHT_MODE);

    let dark = render_template(color, DISCORD_DARK_MODE, bold);
    let light = render_template(color, DISCORD_LIGHT_MODE, bold);

    let contrast_light = contrast_rgb(color, DISCORD_LIGHT_MODE);
    let contrast_dark = contrast_rgb(color, DISCORD_DARK_MODE);

    let handle = ctx.send(|m| m.content("*calculating...*")).await?.unwrap();

    if let poise::ReplyHandle::Application { http, interaction } = handle {
        interaction
            .create_followup_message(http, |m| {
                m.content(format!(
                    "**specified color**\ncontrast on dark mode: {:.2}\ncontrast on light mode: {:.2}",
                    cur_contrast_dark, cur_contrast_light
                )).files([
                    (&cur_light[..], "light_mode.png"),
                    (&cur_dark[..], "dark_mode.png"),
                ])
            })
            .await?;

        interaction
            .create_followup_message(http, |m| {
                m.content(format!(
                "**new color: {}**\ncontrast on dark mode: {:.2}\ncontrast on light mode: {:.2}",
                HexColor::new(color[0],color[1],color[2]),
                contrast_dark,
                contrast_light
                ))
                .files([(&light[..], "light_mode.png"), (&dark[..], "dark_mode.png")])
            })
            .await?;
    }

    Ok(())
}

#[tokio::main]
async fn main() {
    poise::Framework::build()
        .token(std::env::var("DISCORD_TOKEN").unwrap())
        .user_data_setup(move |ctx, _ready, framework| {
            Box::pin(async move {
                let mut commands_builder = serenity::CreateApplicationCommands::default();
                let commands = &framework.options().commands;
                for command in commands {
                    if let Some(slash_command) = command.create_as_slash_command() {
                        commands_builder.add_application_command(slash_command);
                    }
                    if let Some(context_menu_command) = command.create_as_context_menu_command() {
                        commands_builder.add_application_command(context_menu_command);
                    }
                }

                let commands_builder = serenity::json::Value::Array(commands_builder.0);
                println!("registering {} commands as global", commands.len());

                ctx.http
                    .create_global_application_commands(&commands_builder)
                    .await?;

                Ok(())
            })
        })
        .options(poise::FrameworkOptions {
            owners: HashSet::from([serenity::UserId(722196443514798181)]),
            commands: vec![fix(), match_color()],
            on_error: |error| Box::pin(on_error(error)),
            prefix_options: poise::PrefixFrameworkOptions {
                prefix: Some("~kanamori".into()),
                ..Default::default()
            },
            ..Default::default()
        })
        .run()
        .await
        .unwrap();
}

async fn on_error(error: poise::FrameworkError<'_, Data, Error>) {
    println!("aw damnit {:#?}", error);
}
