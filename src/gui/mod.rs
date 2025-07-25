mod find_string;
mod message;
mod named_combobox;
mod request_counter;
mod toggle_switch;

//#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release

use std::collections::{BTreeMap, BTreeSet};
use std::time::{SystemTime};
use std::{
    collections::{HashMap, HashSet},
    ops::DerefMut,
    path::PathBuf,
};

use anyhow::{anyhow, Context, Result};
use eframe::egui::{Button, CollapsingHeader, RichText};
use eframe::epaint::{Pos2, Vec2};
use eframe::{
    egui::{self, FontSelection, Layout, TextFormat, Ui},
    emath::{Align, Align2},
    epaint::{text::LayoutJob, Color32, Stroke},
};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};
use image::GenericImageView;
use tokio::{
    sync::mpsc::{self, Receiver, Sender},
    task::JoinHandle,
};
use tracing::{debug, info, trace};

use crate::mod_lints::{LintId, LintReport, SplitAssetPair};
use crate::{
    integrate::uninstall,
    is_drg_pak,
    providers::{
        ApprovalStatus, FetchProgress, ModInfo, ModSpecification, ModStore, ModioTags,
        ProviderFactory, RequiredStatus,
    },
    state::{ModConfig, ModData_v0_1_0 as ModData, ModOrGroup, ModProfile, State},
};
use find_string::FindString;
use message::MessageHandle;
use request_counter::{RequestCounter, RequestID};

use self::toggle_switch::toggle_switch;

const APP_ICON: &[u8] = include_bytes!("../../assets/icon-gui.ico");

pub fn gui(args: Option<Vec<String>>) -> Result<()> {
    // Decode ICO file
    let app_ico = image::load_from_memory(APP_ICON).expect("failed to load app icon");
    let (_width, _height) = app_ico.dimensions();
    let app_ico_data = app_ico.into_rgba8().into_raw();
    // Set icon
    let options = eframe::NativeOptions {
        centered: true,
        initial_window_size: Some(egui::vec2(900.0, 500.0)),
        min_window_size: Some(egui::vec2(630.0, 175.0)),
        drag_and_drop_support: true,
        icon_data: Some(eframe::IconData {
            rgba: app_ico_data,
            width: 32,
            height: 32,
        }),
        ..Default::default()
    };
    eframe::run_native(
        &format!("mint-notag {}", env!("CARGO_PKG_VERSION")),
        options,
        Box::new(|_cc| Box::new(App::new(args).unwrap())),
    )
    .map_err(|e| anyhow!("{e}"))?;
    Ok(())
}

pub mod colors {
    use eframe::epaint::Color32;

    pub const DARK_RED: Color32 = Color32::DARK_RED;
    pub const DARKER_RED: Color32 = Color32::from_rgb(110, 0, 0);

    pub const DARK_GREEN: Color32 = Color32::DARK_GREEN;
    pub const DARKER_GREEN: Color32 = Color32::from_rgb(0, 80, 0);
}

const FILE_ICO_PNG: &[u8] = include_bytes!("../../assets/folder.png");
const HTTP_ICO_PNG: &[u8] = include_bytes!("../../assets/globe.png");
const MODIO_LOGO_PNG: &[u8] = include_bytes!("../../assets/modio-cog-blue.png");

pub struct App {
    args: Option<Vec<String>>,
    tx: Sender<message::Message>,
    rx: Receiver<message::Message>,
    state: State,
    resolve_mod: String,
    resolve_mod_rid: Option<MessageHandle<()>>,
    integrate_rid: Option<MessageHandle<HashMap<ModSpecification, SpecFetchProgress>>>,
    update_rid: Option<MessageHandle<()>>,
    check_updates_rid: Option<MessageHandle<()>>,
    checked_updates_initially: bool,
    request_counter: RequestCounter,
    window_provider_parameters: Option<WindowProviderParameters>,
    search_string: Option<String>,
    scroll_to_match: bool,
    settings_window: Option<WindowSettings>,
    about_window: Option<WindowAbout>,
    header_texture_handle: Option<egui::TextureHandle>,
    file_texture_handle: Option<egui::TextureHandle>,
    http_texture_handle: Option<egui::TextureHandle>,
    modio_texture_handle: Option<egui::TextureHandle>,
    last_action_status: LastActionStatus,
    available_update: Option<GitHubRelease>,
    show_update_time: Option<SystemTime>,
    open_profiles: HashSet<String>,
    lint_rid: Option<MessageHandle<()>>,
    lint_report_window: Option<WindowLintReport>,
    lint_report: Option<LintReport>,
    lints_toggle_window: Option<WindowLintsToggle>,
    lint_options: LintOptions,
    cache: CommonMarkCache,
    confirm_remove: Option<usize>,
}

#[derive(Default)]
struct LintOptions {
    archive_with_multiple_paks: bool,
    archive_with_only_non_pak_files: bool,
    asset_register_bin: bool,
    conflicting: bool,
    empty_archive: bool,
    outdated_pak_version: bool,
    shader_files: bool,
    non_asset_files: bool,
    split_asset_pairs: bool,
    unmodified_game_assets: bool,
}

enum LastActionStatus {
    Idle,
    Success(String),
    Failure(String),
}

impl App {
    fn new(args: Option<Vec<String>>) -> Result<Self> {
        let (tx, rx) = mpsc::channel(10);
        let state = State::init()?;
        info!("config dir = {}", state.project_dirs.config_dir().display());
        info!("cache dir = {}", state.project_dirs.cache_dir().display());

        Ok(Self {
            args,
            tx,
            rx,
            request_counter: Default::default(),
            state,
            resolve_mod: Default::default(),
            resolve_mod_rid: None,
            integrate_rid: None,
            update_rid: None,
            check_updates_rid: None,
            checked_updates_initially: false,
            window_provider_parameters: None,
            search_string: Default::default(),
            scroll_to_match: false,
            settings_window: None,
            about_window: None,
            header_texture_handle: None,
            file_texture_handle: None,
            http_texture_handle: None,
            modio_texture_handle: None,
            last_action_status: LastActionStatus::Idle,
            available_update: None,
            show_update_time: None,
            open_profiles: Default::default(),
            lint_rid: None,
            lint_report_window: None,
            lint_report: None,
            lints_toggle_window: None,
            lint_options: LintOptions::default(),
            cache: Default::default(),
            confirm_remove: None,
        })
    }

    fn ui_profile(&mut self, ui: &mut Ui, profile: &str) {
        let ModData {
            profiles, groups, ..
        } = self.state.mod_data.deref_mut().deref_mut();

        struct Ctx {
            needs_save: bool,
            scroll_to_match: bool,
            btn_remove: Option<usize>,
            add_deps: Option<Vec<ModSpecification>>,
        }
        let mut ctx = Ctx {
            needs_save: false,
            scroll_to_match: self.scroll_to_match,
            btn_remove: None,
            add_deps: None,
        };

        let mut ui_profile = |ui: &mut Ui, profile: &mut ModProfile| {
            let enabled_specs = profile
                .mods
                .iter()
                .enumerate()
                .flat_map(|(i, m)| -> Box<dyn Iterator<Item = _>> {
                    match m {
                        ModOrGroup::Individual(mc) => {
                            Box::new(mc.enabled.then_some((Some(i), mc.spec.clone())).into_iter())
                        }
                        ModOrGroup::Group {
                            group_name,
                            enabled,
                        } => Box::new(
                            enabled
                                .then(|| groups.get(group_name))
                                .flatten()
                                .into_iter()
                                .flat_map(|g| {
                                    g.mods
                                        .iter()
                                        .filter_map(|m| m.enabled.then_some((None, m.spec.clone())))
                                }),
                        ),
                    }
                })
                .collect::<Vec<_>>();

            let ui_mod_tags = |ctx: &mut Ctx, ui: &mut Ui, info: &ModInfo| {
                if let Some(ModioTags {
                    qol,
                    gameplay,
                    audio,
                    visual,
                    framework,
                    required_status,
                    approval_status,
                    versions: _,
                }) = info.modio_tags.as_ref()
                {
                    let mut mk_searchable_modio_tag =
                        |tag_str: &str,
                         ui: &mut Ui,
                         color: Option<egui::Color32>,
                         hover_str: Option<&str>| {
                            let text_color = if color.is_some() {
                                Color32::BLACK
                            } else {
                                Color32::GRAY
                            };
                            let mut job = LayoutJob::default();
                            let mut is_match = false;
                            if let Some(search_string) = &self.search_string {
                                for (m, chunk) in
                                    find_string::FindString::new(tag_str, search_string)
                                {
                                    let background = if m {
                                        is_match = true;
                                        TextFormat {
                                            background: Color32::YELLOW,
                                            color: text_color,
                                            ..Default::default()
                                        }
                                    } else {
                                        TextFormat {
                                            color: text_color,
                                            ..Default::default()
                                        }
                                    };
                                    job.append(chunk, 0.0, background);
                                }
                            } else {
                                job.append(
                                    tag_str,
                                    0.0,
                                    TextFormat {
                                        color: text_color,
                                        ..Default::default()
                                    },
                                );
                            }

                            let button = if let Some(color) = color {
                                egui::Button::new(job)
                                    .small()
                                    .fill(color)
                                    .stroke(egui::Stroke::NONE)
                            } else {
                                egui::Button::new(job).small().stroke(egui::Stroke::NONE)
                            };

                            let res = if let Some(hover_str) = hover_str {
                                ui.add_enabled(false, button)
                                    .on_disabled_hover_text(hover_str)
                            } else {
                                ui.add_enabled(false, button)
                            };

                            if is_match && self.scroll_to_match {
                                res.scroll_to_me(None);
                                ctx.scroll_to_match = false;
                            }
                        };

                    match approval_status {
                        ApprovalStatus::Verified => {
                            mk_searchable_modio_tag(
                                "V",
                                ui,
                                Some(egui::Color32::LIGHT_GREEN),
                                Some("Client-side mod. Doesn't change gameplay elements."),
                            );
                        }
                        ApprovalStatus::Approved => {
                            mk_searchable_modio_tag(
                                "A",
                                ui,
                                Some(egui::Color32::LIGHT_BLUE),
                                Some("Changes to gameplay elements."),
                            );
                        }
                        ApprovalStatus::Sandbox => {
                            mk_searchable_modio_tag(
                                "S",
                                ui,
                                Some(egui::Color32::LIGHT_YELLOW),
                                Some("Dramatically changes gameplay elements,\nawaiting approval or pak was manually added."),
                            );
                        }
                    }

                    match required_status {
                        RequiredStatus::RequiredByAll => {
                            mk_searchable_modio_tag(
                                "R",
                                ui,
                                Some(egui::Color32::LIGHT_RED),
                                Some("All lobby members must use this mod for it to work correctly.",),
                            );
                        }
                        RequiredStatus::Optional => {}
                    }

                    if *qol {
                        mk_searchable_modio_tag("QoL", ui, None, None);
                    }
                    if *gameplay {
                        mk_searchable_modio_tag("Gameplay", ui, None, None);
                    }
                    if *audio {
                        mk_searchable_modio_tag("Audio", ui, None, None);
                    }
                    if *visual {
                        mk_searchable_modio_tag("Visual", ui, None, None);
                    }
                    if *framework {
                        mk_searchable_modio_tag("Framework", ui, None, None);
                    }
                }
            };

            let mut ui_mod = |ctx: &mut Ctx,
                              ui: &mut Ui,
                              _group: Option<&str>,
                              state: egui_dnd::ItemState,
                              mc: &mut ModConfig| {
                if !mc.enabled {
                    let vis = ui.visuals_mut();
                    vis.override_text_color = Some(vis.text_color());
                    vis.hyperlink_color = vis.text_color();
                }

                if ui
                    .add(toggle_switch(&mut mc.enabled))
                    .on_hover_text_at_pointer(
                        "Enable\\Disable mod.\n\
                        Press \"Apply changes\" for the changes to take effect.",
                    )
                    .changed()
                {
                    ctx.needs_save = true;
                }

                let info = self.state.store.get_mod_info(&mc.spec);

                if mc.enabled {
                    if let Some(req) = &self.integrate_rid {
                        match req.state.get(&mc.spec) {
                            Some(SpecFetchProgress::Progress { progress, size }) => {
                                ui.add(
                                    egui::ProgressBar::new(*progress as f32 / *size as f32)
                                        .show_percentage()
                                        .desired_width(100.0),
                                );
                            }
                            Some(SpecFetchProgress::Complete) => {
                                ui.add(egui::ProgressBar::new(1.0).desired_width(100.0));
                            }
                            None => {
                                ui.spinner();
                            }
                        }
                    }
                }

                if let Some(info) = &info {
                    egui::ComboBox::from_id_source(state.index)
                        .width(20.)
                        .selected_text(
                            self.state
                                .store
                                .get_version_name(&mc.spec)
                                .unwrap_or_default(),
                        )
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut mc.spec.url,
                                info.spec.url.to_string(),
                                self.state
                                    .store
                                    .get_version_name(&info.spec)
                                    .unwrap_or_default(),
                            );
                            for version in info.versions.iter().rev() {
                                ui.selectable_value(
                                    &mut mc.spec.url,
                                    version.url.to_string(),
                                    self.state
                                        .store
                                        .get_version_name(version)
                                        .unwrap_or_default(),
                                );
                            }
                        });

                    if ui
                        .button("📋")
                        .on_hover_text_at_pointer("Copy URL.")
                        .clicked()
                    {
                        ui.output_mut(|o| o.copied_text = mc.spec.url.to_owned());
                    }

                    if mc.enabled {
                        let is_duplicate = enabled_specs.iter().any(|(i, spec)| {
                            Some(state.index) != *i && info.spec.satisfies_dependency(spec)
                        });
                        if is_duplicate
                            && ui
                                .button(
                                    egui::RichText::new("\u{26A0}")
                                        .color(ui.visuals().warn_fg_color),
                                )
                                .on_hover_text_at_pointer("remove duplicate")
                                .clicked()
                        {
                            ctx.btn_remove = Some(state.index);
                        }

                        let missing_deps = info
                            .suggested_dependencies
                            .iter()
                            .filter(|d| {
                                !enabled_specs.iter().any(|(_, s)| s.satisfies_dependency(d))
                            })
                            .collect::<Vec<_>>();

                        if !missing_deps.is_empty() {
                            let mut msg = "Add missing dependencies:".to_string();
                            for dep in &missing_deps {
                                msg.push('\n');
                                msg.push_str(&dep.url);
                            }
                            if ui
                                .button(
                                    egui::RichText::new("\u{26A0}")
                                        .color(ui.visuals().warn_fg_color),
                                )
                                .on_hover_text(msg)
                                .clicked()
                            {
                                ctx.add_deps = Some(missing_deps.into_iter().cloned().collect());
                            }
                        }
                    }

                    let mut job = LayoutJob::default();
                    let mut is_match = false;
                    if let Some(search_string) = &self.search_string {
                        for (m, chunk) in FindString::new(&info.name, search_string) {
                            let background = if m {
                                is_match = true;
                                TextFormat {
                                    background: Color32::YELLOW,
                                    ..Default::default()
                                }
                            } else {
                                Default::default()
                            };
                            job.append(chunk, 0.0, background);
                        }
                    } else {
                        job.append(&info.name, 0.0, Default::default());
                    }

                    match info.provider {
                        "modio" => {
                            let texture: &egui::TextureHandle =
                                self.modio_texture_handle.get_or_insert_with(|| {
                                    let image = image::load_from_memory(MODIO_LOGO_PNG).unwrap();
                                    let size = [image.width() as _, image.height() as _];
                                    let image_buffer = image.to_rgba8();
                                    let pixels = image_buffer.as_flat_samples();
                                    let image = egui::ColorImage::from_rgba_unmultiplied(
                                        size,
                                        pixels.as_slice(),
                                    );

                                    ui.ctx()
                                        .load_texture("modio-logo", image, Default::default())
                                });
                            let mut img = egui::Image::new(texture, [16.0, 16.0]);
                            if !mc.enabled {
                                img = img.tint(Color32::LIGHT_RED);
                            }
                            ui.add(img);
                        }
                        "http" => {
                            let texture: &egui::TextureHandle =
                                self.http_texture_handle.get_or_insert_with(|| {
                                    let image = image::load_from_memory(HTTP_ICO_PNG).unwrap();
                                    let size = [image.width() as _, image.height() as _];
                                    let image_buffer = image.to_rgba8();
                                    let pixels = image_buffer.as_flat_samples();
                                    let image = egui::ColorImage::from_rgba_unmultiplied(
                                        size,
                                        pixels.as_slice(),
                                    );

                                    ui.ctx()
                                        .load_texture("http-logo", image, Default::default())
                                });
                            let mut img = egui::Image::new(texture, [16.0, 16.0]);
                            if !mc.enabled {
                                img = img.tint(Color32::LIGHT_RED);
                            }
                            ui.add(img);
                        }
                        "file" => {
                            let texture: &egui::TextureHandle =
                                self.file_texture_handle.get_or_insert_with(|| {
                                    let image = image::load_from_memory(FILE_ICO_PNG).unwrap();
                                    let size = [image.width() as _, image.height() as _];
                                    let image_buffer = image.to_rgba8();
                                    let pixels = image_buffer.as_flat_samples();
                                    let image = egui::ColorImage::from_rgba_unmultiplied(
                                        size,
                                        pixels.as_slice(),
                                    );

                                    ui.ctx()
                                        .load_texture("file-logo", image, Default::default())
                                });
                            let mut img = egui::Image::new(texture, [16.0, 16.0]);
                            if !mc.enabled {
                                img = img.tint(Color32::LIGHT_RED);
                            }
                            ui.add(img);
                        }
                        _ => unimplemented!("unimplemented provider kind"),
                    }

                    let res = ui.hyperlink_to(job, &mc.spec.url);
                    if is_match && self.scroll_to_match {
                        res.scroll_to_me(None);
                        ctx.scroll_to_match = false;
                    }

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui_mod_tags(ctx, ui, info);
                    });
                } else {
                    if ui
                        .button("📋")
                        .on_hover_text_at_pointer("Copy URL.")
                        .clicked()
                    {
                        ui.output_mut(|o| o.copied_text = mc.spec.url.to_owned());
                    }
                    ui.hyperlink(&mc.spec.url);
                }
            };

            let mut ui_item =
                |ctx: &mut Ctx, ui: &mut Ui, mc: &mut ModOrGroup, state: egui_dnd::ItemState| {
                    ui.scope(|ui| {
                        ui.visuals_mut().widgets.hovered.weak_bg_fill = colors::DARK_RED;
                        ui.visuals_mut().widgets.active.weak_bg_fill = colors::DARKER_RED;
                        if ui
                            .add(Button::new(" 🗑 "))
                            .on_hover_text_at_pointer("Delete mod")
                            .clicked()
                        {
                            self.confirm_remove = Some(state.index);
                        };
                    });

                    match mc {
                        ModOrGroup::Individual(mc) => {
                            ui_mod(ctx, ui, None, state, mc);
                        }
                        ModOrGroup::Group {
                            ref group_name,
                            enabled,
                        } => {
                            if ui
                                .add(toggle_switch(enabled))
                                .on_hover_text_at_pointer("Enabled?")
                                .changed()
                            {
                                ctx.needs_save = true;
                            }
                            ui.collapsing(group_name, |ui| {
                                for (index, m) in groups
                                    .get_mut(group_name)
                                    .unwrap()
                                    .mods
                                    .iter_mut()
                                    .enumerate()
                                {
                                    ui.horizontal(|ui| {
                                        ui_mod(
                                            ctx,
                                            ui,
                                            Some(group_name),
                                            egui_dnd::ItemState {
                                                index,
                                                dragged: false,
                                            },
                                            m,
                                        )
                                    });
                                }
                            });
                        }
                    }
                };

            let res = egui_dnd::dnd(ui, ui.id()).show(
                profile.mods.iter_mut().enumerate(),
                |ui, (_index, item), handle, state| {
                    let mut frame = egui::Frame::none();
                    if state.dragged {
                        frame.fill = ui.visuals().extreme_bg_color
                    } else if state.index % 2 == 1 {
                        frame.fill = ui.visuals().faint_bg_color
                    }
                    frame.show(ui, |ui| {
                        ui.horizontal(|ui| {
                            handle.ui(ui, |ui| {
                                ui.label("   ☰  ");
                            });

                            ui_item(&mut ctx, ui, item, state);
                        });
                    });
                },
            );

            if res.final_update().is_some() {
                res.update_vec(&mut profile.mods);
                ctx.needs_save = true;
            }

            if let Some(index) = self.confirm_remove {
                egui::Window::new("Delete mod")
                    .collapsible(false)
                    .resizable(false)
                    .show(ui.ctx(), |ui| {
                        ui.add_space(10.);
                        ui.label("Are you sure you want to delete this mod from your load order? This cannot be undone.");
                        ui.add_space(10.);

                        ui.with_layout(egui::Layout::right_to_left(Align::TOP), |ui| {
                            if ui.button("Cancel").clicked() {
                                self.confirm_remove = None;
                            }
                            if ui.button("Delete").clicked() {
                                profile.mods.remove(index);
                                ctx.needs_save = true;
                                self.confirm_remove = None;
                            }
                        });
                    });
            }
        };

        egui::ScrollArea::vertical().show(ui, |ui| {
            if let Some(profile) = profiles.get_mut(profile) {
                ui_profile(ui, profile);
            } else {
                ui.label("no such profile");
            }
        });

        if let Some(add_deps) = ctx.add_deps {
            message::ResolveMods::send(self, ui.ctx(), add_deps, true);
        }

        self.scroll_to_match = ctx.scroll_to_match;

        if ctx.needs_save {
            self.state.mod_data.save().unwrap();
        }
    }

    fn parse_mods(&self) -> Vec<ModSpecification> {
        self.resolve_mod
            .lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty())
            .map(|l| ModSpecification::new(l.to_string()))
            .collect()
    }

    fn build_mod_string(mods: &Vec<ModConfig>) -> String {
        let mut string = String::new();
        for m in mods {
            if m.enabled {
                string.push_str(&m.spec.url);
                string.push('\n');
            }
        }
        string
    }

    fn show_update_window(&mut self, ctx: &egui::Context) {
        if let (Some(update), Some(_update_time)) =
            (self.available_update.as_ref(), self.show_update_time)
        {
            // Backdrop
            egui::Area::new("available-update-overlay")
                .movable(false)
                .fixed_pos(Pos2::ZERO)
                .order(egui::Order::Background)
                .show(ctx, |ui| {
                    egui::Frame::none()
                        .fill(Color32::from_rgba_unmultiplied(0, 0, 0, 127))
                        .show(ui, |ui| {
                            ui.allocate_space(ui.available_size());
                        })
                });
            // Actual update window
            egui::Window::new(format!("Update Available: {}", update.tag_name))
                .collapsible(false)
                .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
                .resizable(false)
                .show(ctx, |ui| {
                    // https://docs.rs/egui_commonmark/latest/egui_commonmark/struct.CommonMarkViewer.html
                    CommonMarkViewer::new("available-update")
                        .max_image_width(Some(512))
                        .show_alt_text_on_hover(false)
                        .show(ui, &mut self.cache, &update.body);
                    ui.with_layout(egui::Layout::right_to_left(Align::TOP), |ui| {
                        if ui.button("Close").clicked() {
                            self.show_update_time = None;
                        }
                        if ui.button("Download").clicked() {
                            ui.ctx().output_mut(|o| {
                                o.open_url = Some(egui::output::OpenUrl {
                                    url: update.html_url.clone(),
                                    new_tab: true,
                                });
                            });
                        }
                    });
                });
        }
    }

    fn show_provider_parameters(&mut self, ctx: &egui::Context) {
        let Some(window) = &mut self.window_provider_parameters else {
            return;
        };

        while let Ok((rid, res)) = window.rx.try_recv() {
            if window.check_rid.as_ref().map_or(false, |r| rid == r.0) {
                match res {
                    Ok(()) => {
                        let window = self.window_provider_parameters.take().unwrap();
                        self.state
                            .config
                            .provider_parameters
                            .insert(window.factory.id.to_string(), window.parameters);
                        self.state.config.save().unwrap();
                        return;
                    }
                    Err(e) => {
                        window.check_error = Some(e.to_string());
                    }
                }
                window.check_rid = None;
            }
        }

        let mut open = true;
        let mut check = false;
        egui::Window::new(format!("Configure {} provider", window.factory.id))
            .open(&mut open)
            .resizable(false)
            .show(ctx, |ui| {
                ui.add_enabled_ui(window.check_rid.is_none(), |ui| {
                    egui::Grid::new("grid").num_columns(2).show(ui, |ui| {
                        for p in window.factory.parameters {
                            if let Some(link) = p.link {
                                ui.hyperlink_to(p.name, link).on_hover_text(p.description);
                            } else {
                                ui.label(p.name).on_hover_text(p.description);
                            }
                            let res = ui.add(
                                egui::TextEdit::singleline(
                                    window.parameters.entry(p.id.to_string()).or_default(),
                                )
                                .password(true)
                                .desired_width(200.0),
                            );
                            if is_committed(&res) {
                                check = true;
                            }
                            ui.end_row();
                        }
                    });

                    ui.with_layout(Layout::right_to_left(Align::TOP), |ui| {
                        if ui.button("Save").clicked() {
                            check = true;
                        }
                        if window.check_rid.is_some() {
                            ui.spinner();
                        }
                        if let Some(error) = &window.check_error {
                            ui.colored_label(ui.visuals().error_fg_color, error);
                        }
                    });
                });
            });
        if !open {
            self.window_provider_parameters = None;
        } else if check {
            window.check_error = None;
            let tx = window.tx.clone();
            let ctx = ctx.clone();
            let rid = self.request_counter.next();
            let store = self.state.store.clone();
            let params = window.parameters.clone();
            let factory = window.factory;
            let handle = tokio::task::spawn(async move {
                let res = store.add_provider_checked(factory, &params).await;
                tx.send((rid, res)).await.unwrap();
                ctx.request_repaint();
            });
            window.check_rid = Some((rid, handle));
        }
    }

    fn show_profile_windows(&mut self, ctx: &egui::Context) {
        let mut to_remove = vec![];
        for profile in &self.open_profiles.clone() {
            let mut open = true;
            egui::Window::new(format!("Profile \"{profile}\""))
                .open(&mut open)
                .show(ctx, |ui| {
                    self.ui_profile(ui, profile);
                });
            if !open {
                to_remove.push(profile.clone());
            }
        }
        for r in to_remove {
            self.open_profiles.remove(&r);
        }
    }

    fn show_settings(&mut self, ctx: &egui::Context) {
        if let Some(window) = &mut self.settings_window {
            let mut open = true;
            let mut try_save = false;
            egui::Window::new("Settings")
                .open(&mut open)
                .resizable(false)
                .show(ctx, |ui| {
                    egui::Grid::new("grid").num_columns(2).show(ui, |ui| {
                        ui.heading("Game data");
                        ui.end_row();

                        let mut job = LayoutJob::default();
                        job.append(
                            "pak directory:",
                            0.0,
                            TextFormat {
                                color: ui.visuals().text_color(),
                                underline: Stroke::new(0.8, ui.visuals().text_color()),
                                ..Default::default()
                            },
                        );
                        ui.label(job).on_hover_cursor(egui::CursorIcon::Help).on_hover_text(
                            "Path to \"FSD-WindowsNoEditor.pak\" (\"FSD-WinGDK.pak\" for the Microsoft Store version).\n\
                            The pak is located inside <Deep Rock Galactic installation directory>/FSD/Content/Paks."
                        );
                        ui.horizontal(|ui| {
                            let res = ui.add(
                                egui::TextEdit::singleline(
                                    &mut window.drg_pak_path
                                )
                                .desired_width(300.),
                            );
                            if res.changed() {
                                window.drg_pak_path_err = None;
                            }
                            if is_committed(&res) {
                                try_save = true;
                            }
                            if ui.button("Browse...").clicked() {
                                if let Some(fsd_pak) = rfd::FileDialog::new()
                                    .add_filter("DRG Pak", &["pak"])
                                    .pick_file()
                                {
                                    window.drg_pak_path = fsd_pak.to_string_lossy().to_string();
                                    window.drg_pak_path_err = None;
                                }
                            }
                        });
                        ui.end_row();

                        ui.add_space(1.);
                        ui.end_row();

                        ui.heading("App data");
                        ui.end_row();

                        let config_dir = self.state.project_dirs.config_dir();
                        ui.label("Config directory:");
                        if ui.link(config_dir.display().to_string()).clicked() {
                            opener::open(config_dir).ok();
                        }
                        ui.end_row();

                        let cache_dir = self.state.project_dirs.cache_dir();
                        ui.label("Cache directory:");
                        if ui.link(cache_dir.display().to_string()).clicked() {
                            opener::open(cache_dir).ok();
                        }
                        ui.end_row();

                        ui.add_space(1.);
                        ui.end_row();

                        ui.heading("Mod providers");
                        ui.end_row();

                        for provider_factory in ModStore::get_provider_factories() {
                            ui.label(provider_factory.id);
                            if ui.add_enabled(!provider_factory.parameters.is_empty(), egui::Button::new(" ⚙ "))
                                    .on_hover_text(format!("Open \"{}\" provider settings.", provider_factory.id))
                                    .clicked() {
                                self.window_provider_parameters = Some(
                                    WindowProviderParameters::new(provider_factory, &self.state),
                                );
                            }
                            ui.end_row();
                        }
                    });

                    ui.with_layout(egui::Layout::right_to_left(Align::TOP), |ui| {
                        if ui.add_enabled(window.drg_pak_path_err.is_none(), egui::Button::new("Save")).clicked() {
                            try_save = true;
                        }
                        if let Some(error) = &window.drg_pak_path_err {
                            ui.colored_label(ui.visuals().error_fg_color, error);
                        }
                    });

                });
            if try_save {
                if let Err(e) = is_drg_pak(&window.drg_pak_path).context("Selected DRG pak is not valid!") {
                    window.drg_pak_path_err = Some(e.to_string());
                } else {
                    self.state.config.drg_pak_path = Some(PathBuf::from(
                        self.settings_window.take().unwrap().drg_pak_path,
                    ));
                    self.state.config.save().unwrap();
                }
            } else if !open {
                self.settings_window = None;
            }
        }
    }

    fn show_about(&mut self, ctx: &egui::Context) {
        if let Some(_) = &mut self.about_window {
            let mut open = true;

            egui::Window::new("About")
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {

                    // Load logo image
                    let header_texture: &egui::TextureHandle = self.header_texture_handle.get_or_insert_with(|| {
                        let image = image::load_from_memory(include_bytes!("../../assets/header.png")).unwrap();
                        let size = [image.width() as _, image.height() as _];
                        let image_buffer = image.to_rgba8();
                        let pixels = image_buffer.as_flat_samples();
                        let image = egui::ColorImage::from_rgba_unmultiplied(size, pixels.as_slice());

                        ui.ctx().load_texture("header-image", image, Default::default())
                    });

                    ui.image(header_texture, header_texture.size_vec2());
                    ui.add_space(10.);
                    ui.label(format!("mint-notag v{}", env!("CARGO_PKG_VERSION")));
                    // ui.label("Third-party mod integration tool for Deep Rock Galactic.");
                    ui.label(
                        "Fork of mint that - among other small changes - omits\n\
                        the \"[MODDED]\" prefix from the public lobby name."
                    );
                    ui.add_space(10.);
                    ui.hyperlink_to("Changelog", "https://github.com/Strappazzon/drg-mint-notag/blob/-/CHANGELOG.md");
                    ui.hyperlink_to("License", "https://github.com/Strappazzon/drg-mint-notag/blob/-/LICENSE.txt");
                    ui.hyperlink_to("GitHub", "https://github.com/Strappazzon/drg-mint-notag");
                    ui.add_space(10.);
                    // https://github.com/emilk/egui/discussions/1495#discussioncomment-2571140
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        ui.label("Application icon from ");
                        ui.hyperlink_to("Icons8", "https://icons8.com/icon/6zZTdWRZoWil/rock");
                        ui.label(".");
                    });
                    ui.add_space(10.);

                    ui.with_layout(egui::Layout::right_to_left(Align::TOP), |ui| {
                        if ui.button("Close").clicked() {
                            self.about_window = None;
                        }
                    });
                });

            if !open {
                self.about_window = None;
            }
        }
    }

    fn show_lints_toggle(&mut self, ctx: &egui::Context) {
        if let Some(lints_toggle) = &self.lints_toggle_window {
            let mut open = true;

            let mods = lints_toggle.mods.clone();

            egui::Window::new("Toggle lints")
                .open(&mut open)
                .resizable(false)
                .show(ctx, |ui| {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        egui::Grid::new("lints-toggle-grid").show(ui, |ui| {
                            ui.heading("Lint");
                            ui.heading("Toggle");
                            ui.end_row();

                            ui.label("Archive with multiple paks");
                            ui.add(toggle_switch(
                                &mut self.lint_options.archive_with_multiple_paks,
                            ));
                            ui.end_row();

                            ui.label("Archive with only non-pak files");
                            ui.add(toggle_switch(
                                &mut self.lint_options.archive_with_only_non_pak_files,
                            ));
                            ui.end_row();

                            ui.label("Mods containing AssetRegister.bin");
                            ui.add(toggle_switch(&mut self.lint_options.asset_register_bin));
                            ui.end_row();

                            ui.label("Mods containing conflicting files");
                            ui.add(toggle_switch(&mut self.lint_options.conflicting));
                            ui.end_row();

                            ui.label("Mods containing empty archives");
                            ui.add(toggle_switch(&mut self.lint_options.empty_archive));
                            ui.end_row();

                            ui.label("Mods containing oudated pak version");
                            ui.add(toggle_switch(&mut self.lint_options.outdated_pak_version));
                            ui.end_row();

                            ui.label("Mods containing shader files");
                            ui.add(toggle_switch(&mut self.lint_options.shader_files));
                            ui.end_row();

                            ui.label("Mods containing non-asset files");
                            ui.add(toggle_switch(&mut self.lint_options.non_asset_files));
                            ui.end_row();

                            ui.label("Mods containing split {uexp, uasset} pairs");
                            ui.add(toggle_switch(&mut self.lint_options.split_asset_pairs));
                            ui.end_row();

                            ui.label("Mods containing unmodified game assets");
                            ui.add_enabled(
                                self.state.config.drg_pak_path.is_some(),
                                toggle_switch(&mut self.lint_options.unmodified_game_assets),
                            )
                            .on_disabled_hover_text(
                                "This lint requires DRG pak path to be specified",
                            );
                            ui.end_row();
                        });
                    });

                    ui.horizontal(|ui| {
                        if ui.button("Cancel").clicked() {
                            self.lints_toggle_window = None;
                        }

                        if ui
                            .add_enabled(
                                self.check_updates_rid.is_none()
                                    && self.integrate_rid.is_none()
                                    && self.lint_rid.is_none(),
                                egui::Button::new("Generate report"),
                            )
                            .clicked()
                        {
                            let lint_options = BTreeMap::from([
                                (
                                    LintId::ARCHIVE_WITH_MULTIPLE_PAKS,
                                    self.lint_options.archive_with_multiple_paks,
                                ),
                                (
                                    LintId::ARCHIVE_WITH_ONLY_NON_PAK_FILES,
                                    self.lint_options.archive_with_only_non_pak_files,
                                ),
                                (
                                    LintId::ASSET_REGISTRY_BIN,
                                    self.lint_options.asset_register_bin,
                                ),
                                (LintId::CONFLICTING, self.lint_options.conflicting),
                                (LintId::EMPTY_ARCHIVE, self.lint_options.empty_archive),
                                (
                                    LintId::OUTDATED_PAK_VERSION,
                                    self.lint_options.outdated_pak_version,
                                ),
                                (LintId::SHADER_FILES, self.lint_options.shader_files),
                                (LintId::NON_ASSET_FILES, self.lint_options.non_asset_files),
                                (
                                    LintId::SPLIT_ASSET_PAIRS,
                                    self.lint_options.split_asset_pairs,
                                ),
                                (
                                    LintId::UNMODIFIED_GAME_ASSETS,
                                    self.lint_options.unmodified_game_assets,
                                ),
                            ]);

                            trace!(?lint_options);

                            self.lint_report = None;
                            self.lint_rid = Some(message::LintMods::send(
                                &mut self.request_counter,
                                self.state.store.clone(),
                                mods,
                                BTreeSet::from_iter(
                                    lint_options
                                        .into_iter()
                                        .filter_map(|(lint, enabled)| enabled.then_some(lint)),
                                ),
                                self.state.config.drg_pak_path.clone(),
                                self.tx.clone(),
                                ctx.clone(),
                            ));

                            self.lint_report_window = Some(WindowLintReport);
                        }
                    });
                });

            if !open {
                self.lints_toggle_window = None;
            }
        }
    }

    fn show_lint_report(&mut self, ctx: &egui::Context) {
        if self.lint_report_window.is_some() {
            let mut open = true;

            egui::Window::new("Lint results")
                .open(&mut open)
                .resizable(true)
                .show(ctx, |ui| {
                    if let Some(report) = &self.lint_report {
                        let scroll_height =
                            (ui.available_height() - 30.0).clamp(0.0, f32::INFINITY);
                        egui::ScrollArea::vertical()
                            .max_height(scroll_height)
                            .show(ui, |ui| {
                                const AMBER: Color32 = Color32::from_rgb(255, 191, 0);

                                if let Some(conflicting_mods) = &report.conflicting_mods {
                                    if !conflicting_mods.is_empty() {
                                        CollapsingHeader::new(
                                            RichText::new("⚠ Mods(s) with conflicting asset modifications detected")
                                                .color(AMBER),
                                        )
                                        .default_open(true)
                                        .show(ui, |ui| {
                                            conflicting_mods.iter().for_each(|(path, mods)| {
                                                CollapsingHeader::new(
                                                    RichText::new(format!(
                                                        "⚠ Conflicting modification of asset `{}`",
                                                        path
                                                    ))
                                                    .color(AMBER),
                                                )
                                                .show(
                                                    ui,
                                                    |ui| {
                                                        mods.iter().for_each(|mod_spec| {
                                                            ui.label(&mod_spec.url);
                                                        });
                                                    },
                                                );
                                            });
                                        });
                                    }
                                }

                                if let Some(asset_register_bin_mods) = &report.asset_register_bin_mods {
                                    if !asset_register_bin_mods.is_empty() {
                                        CollapsingHeader::new(
                                            RichText::new("ℹ Mod(s) with `AssetRegistry.bin` included detected")
                                                .color(Color32::LIGHT_BLUE),
                                        )
                                        .default_open(true)
                                        .show(ui, |ui| {
                                            asset_register_bin_mods.iter().for_each(
                                                |(r#mod, paths)| {
                                                    CollapsingHeader::new(
                                                        RichText::new(format!(
                                                        "ℹ {} includes one or more `AssetRegistry.bin`",
                                                        r#mod.url
                                                    ))
                                                        .color(Color32::LIGHT_BLUE),
                                                    )
                                                    .show(ui, |ui| {
                                                        paths.iter().for_each(|path| {
                                                            ui.label(path);
                                                        });
                                                    });
                                                },
                                            );
                                        });
                                    }
                                }

                                if let Some(shader_file_mods) = &report.shader_file_mods {
                                    if !shader_file_mods.is_empty() {
                                        CollapsingHeader::new(
                                            RichText::new(
                                                "⚠ Mods(s) with shader files included detected",
                                            )
                                            .color(AMBER),
                                        )
                                        .default_open(true)
                                        .show(ui, |ui| {
                                            shader_file_mods.iter().for_each(
                                                |(r#mod, shader_files)| {
                                                    CollapsingHeader::new(
                                                        RichText::new(format!(
                                                            "⚠ {} includes one or more shader files",
                                                            r#mod.url
                                                        ))
                                                        .color(AMBER),
                                                    )
                                                    .show(ui, |ui| {
                                                        shader_files.iter().for_each(|shader_file| {
                                                            ui.label(shader_file);
                                                        });
                                                    });
                                                },
                                            );
                                        });
                                    }
                                }

                                if let Some(outdated_pak_version_mods) = &report.outdated_pak_version_mods {
                                    if !outdated_pak_version_mods.is_empty() {
                                        CollapsingHeader::new(
                                            RichText::new(
                                                "⚠ Mod(s) with outdated pak version detected",
                                            )
                                            .color(AMBER),
                                        )
                                        .default_open(true)
                                        .show(ui, |ui| {
                                            outdated_pak_version_mods.iter().for_each(
                                                |(r#mod, version)| {
                                                    ui.label(
                                                        RichText::new(format!(
                                                            "⚠ {} includes outdated pak version {}",
                                                            r#mod.url, version
                                                        ))
                                                        .color(AMBER),
                                                    );
                                                },
                                            );
                                        });
                                    }
                                }

                                if let Some(empty_archive_mods) = &report.empty_archive_mods {
                                    if !empty_archive_mods.is_empty() {
                                        CollapsingHeader::new(
                                            RichText::new(
                                                "⚠ Mod(s) with empty archives detected",
                                            )
                                            .color(AMBER),
                                        )
                                        .default_open(true)
                                        .show(ui, |ui| {
                                            empty_archive_mods.iter().for_each(|r#mod| {
                                                ui.label(
                                                    RichText::new(format!(
                                                        "⚠ {} contains an empty archive",
                                                        r#mod.url
                                                    ))
                                                    .color(AMBER),
                                                );
                                            });
                                        });
                                    }
                                }

                                if let Some(archive_with_only_non_pak_files_mods) = &report.archive_with_only_non_pak_files_mods {
                                    if !archive_with_only_non_pak_files_mods.is_empty() {
                                        CollapsingHeader::new(
                                            RichText::new(
                                                "⚠ Mod(s) with only non-`.pak` files detected",
                                            )
                                            .color(AMBER),
                                        )
                                        .default_open(true)
                                        .show(ui, |ui| {
                                            archive_with_only_non_pak_files_mods.iter().for_each(|r#mod| {
                                                ui.label(
                                                    RichText::new(format!(
                                                        "⚠ {} contains only non-`.pak` files, perhaps the author forgot to pack it?",
                                                        r#mod.url
                                                    ))
                                                    .color(AMBER),
                                                );
                                            });
                                        });
                                    }
                                }

                                if let Some(archive_with_multiple_paks_mods) = &report.archive_with_multiple_paks_mods {
                                    if !archive_with_multiple_paks_mods.is_empty() {
                                        CollapsingHeader::new(
                                            RichText::new(
                                                "⚠ Mod(s) with multiple `.pak`s detected",
                                            )
                                            .color(AMBER),
                                        )
                                        .default_open(true)
                                        .show(ui, |ui| {
                                            archive_with_multiple_paks_mods.iter().for_each(|r#mod| {
                                                ui.label(RichText::new(format!(
                                                    "⚠ {} contains multiple `.pak`s, only the first encountered `.pak` will be loaded",
                                                    r#mod.url
                                                ))
                                                .color(AMBER));
                                            });
                                        });
                                    }
                                }

                                if let Some(non_asset_file_mods) = &report.non_asset_file_mods {
                                    if !non_asset_file_mods.is_empty() {
                                        CollapsingHeader::new(
                                            RichText::new(
                                                "⚠ Mod(s) with non-asset files detected",
                                            )
                                            .color(AMBER),
                                        )
                                        .default_open(true)
                                        .show(ui, |ui| {
                                            non_asset_file_mods.iter().for_each(|(r#mod, files)| {
                                                CollapsingHeader::new(
                                                    RichText::new(format!(
                                                        "⚠ {} includes non-asset files",
                                                        r#mod.url
                                                    ))
                                                    .color(AMBER),
                                                )
                                                .show(ui, |ui| {
                                                    files.iter().for_each(|file| {
                                                        ui.label(file);
                                                    });
                                                });
                                            });
                                        });
                                    }
                                }

                                if let Some(split_asset_pairs_mods) = &report.split_asset_pairs_mods {
                                    if !split_asset_pairs_mods.is_empty() {
                                        CollapsingHeader::new(
                                            RichText::new(
                                                "⚠ Mod(s) with split {uexp, uasset} pairs detected",
                                            )
                                            .color(AMBER),
                                        )
                                        .default_open(true)
                                        .show(ui, |ui| {
                                            split_asset_pairs_mods.iter().for_each(|(r#mod, files)| {
                                                CollapsingHeader::new(
                                                    RichText::new(format!(
                                                        "⚠ {} includes split {{uexp, uasset}} pairs",
                                                        r#mod.url
                                                    ))
                                                    .color(AMBER),
                                                )
                                                .show(ui, |ui| {
                                                    files.iter().for_each(|(file, kind)| {
                                                        match kind {
                                                            SplitAssetPair::MissingUasset => {
                                                                ui.label(format!("`{file}` missing matching .uasset file"));
                                                            },
                                                            SplitAssetPair::MissingUexp => {
                                                                ui.label(format!("`{file}` missing matching .uexp file"));
                                                            }
                                                        }
                                                    });
                                                });
                                            });
                                        });
                                    }
                                }

                                if let Some(unmodified_game_assets_mods) = &report.unmodified_game_assets_mods {
                                    if !unmodified_game_assets_mods.is_empty() {
                                        CollapsingHeader::new(
                                            RichText::new(
                                                "⚠ Mod(s) with unmodified game assets detected",
                                            )
                                            .color(AMBER),
                                        )
                                        .default_open(true)
                                        .show(ui, |ui| {
                                            unmodified_game_assets_mods.iter().for_each(|(r#mod, files)| {
                                                CollapsingHeader::new(
                                                    RichText::new(format!(
                                                        "⚠ {} includes unmodified game assets",
                                                        r#mod.url
                                                    ))
                                                    .color(AMBER),
                                                )
                                                .show(ui, |ui| {
                                                    files.iter().for_each(|file| {
                                                        ui.label(file);
                                                    });
                                                });
                                            });
                                        });
                                    }
                                }
                            });
                    } else {
                        ui.spinner();
                        ui.label("Lint report generating...");
                    }
                });

            if !open {
                self.lint_report_window = None;
                self.lint_rid = None;
            }
        }
    }
}

struct WindowProviderParameters {
    tx: Sender<(RequestID, Result<()>)>,
    rx: Receiver<(RequestID, Result<()>)>,
    check_rid: Option<(RequestID, JoinHandle<()>)>,
    check_error: Option<String>,
    factory: &'static ProviderFactory,
    parameters: HashMap<String, String>,
}

impl WindowProviderParameters {
    fn new(factory: &'static ProviderFactory, state: &State) -> Self {
        let (tx, rx) = mpsc::channel(10);
        Self {
            tx,
            rx,
            check_rid: None,
            check_error: None,
            parameters: state
                .config
                .provider_parameters
                .get(factory.id)
                .cloned()
                .unwrap_or_default(),
            factory,
        }
    }
}

struct WindowSettings {
    drg_pak_path: String,
    drg_pak_path_err: Option<String>,
}

impl WindowSettings {
    fn new(state: &State) -> Self {
        let path = state
            .config
            .drg_pak_path
            .as_ref()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();
        Self {
            drg_pak_path: path,
            drg_pak_path_err: None,
        }
    }
}

struct WindowAbout;

impl WindowAbout {
    fn new() -> Self {
        Self
    }
}

struct WindowLintReport;

struct WindowLintsToggle {
    mods: Vec<ModSpecification>,
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if !self.checked_updates_initially {
            self.checked_updates_initially = true;
            message::CheckUpdates::send(self, ctx);
        }

        // message handling
        while let Ok(msg) = self.rx.try_recv() {
            msg.handle(self);
        }

        // begin draw

        self.show_update_window(ctx);
        self.show_provider_parameters(ctx);
        self.show_profile_windows(ctx);
        self.show_settings(ctx);
        self.show_about(ctx);
        self.show_lints_toggle(ctx);
        self.show_lint_report(ctx);

        egui::TopBottomPanel::bottom("bottom_panel").show(ctx, |ui| {
            ui.with_layout(egui::Layout::right_to_left(Align::TOP), |ui| {
                ui.add_enabled_ui(
                    self.integrate_rid.is_none()
                        && self.update_rid.is_none()
                        && self.lint_rid.is_none()
                        && self.state.config.drg_pak_path.is_some(),
                    |ui| {
                        if let Some(args) = &self.args {
                            if ui
                                .button("Launch game")
                                .on_hover_ui(|ui| {
                                    for arg in args {
                                        ui.label(arg);
                                    }
                                })
                                .clicked()
                            {
                                let args = args.clone();
                                tokio::task::spawn_blocking(move || {
                                    let mut iter = args.iter();
                                    std::process::Command::new(iter.next().unwrap())
                                        .args(iter)
                                        .spawn()
                                        .unwrap();
                                });
                            }
                        }

                        ui.add_enabled_ui(self.state.config.drg_pak_path.is_some(), |ui| {
                            let mut button = ui.button("Apply changes").on_hover_text(
                                "Install the hook DLL inside the game folder and regenerate mod bundle.",
                            );
                            if self.state.config.drg_pak_path.is_none() {
                                button = button.on_disabled_hover_text(
                                    "Deep Rock Galactic not found.\n\
                                    Set the correct path in the settings menu.",
                                );
                            }

                            let mut mods = Vec::new();
                            let active_profile = self.state.mod_data.active_profile.clone();
                            self.state
                                .mod_data
                                .for_each_enabled_mod(&active_profile, |mc| {
                                    mods.push(mc.spec.clone());
                                });

                            if button.clicked() {
                                self.last_action_status = LastActionStatus::Idle;
                                self.integrate_rid = Some(message::Integrate::send(
                                    &mut self.request_counter,
                                    self.state.store.clone(),
                                    mods,
                                    self.state.config.drg_pak_path.as_ref().unwrap().clone(),
                                    self.tx.clone(),
                                    ctx.clone(),
                                ));
                            }
                        });

                        ui.add_enabled_ui(self.state.config.drg_pak_path.is_some(), |ui| {
                            let mut button = ui.button("Uninstall hook and mods").on_hover_text(
                                "Remove the hook DLL and mod bundle from the game folder.",
                            );
                            if self.state.config.drg_pak_path.is_none() {
                                button = button.on_disabled_hover_text(
                                    "Deep Rock Galactic not found.\n\
                                    Set the correct path in the settings menu.",
                                );
                            }
                            if button.clicked() {
                                self.last_action_status = LastActionStatus::Idle;
                                if let Some(pak_path) = &self.state.config.drg_pak_path {
                                    let mut mods = HashSet::default();
                                    let active_profile = self.state.mod_data.active_profile.clone();
                                    self.state.mod_data.for_each_enabled_mod(
                                        &active_profile,
                                        |mc| {
                                            if let Some(modio_id) = self
                                                .state
                                                .store
                                                .get_mod_info(&mc.spec)
                                                .and_then(|i| i.modio_id)
                                            {
                                                mods.insert(modio_id);
                                            }
                                        },
                                    );

                                    debug!("uninstalling mods: pak_path = {}", pak_path.display());
                                    match uninstall(pak_path, mods) {
                                        Ok(()) => {
                                            self.last_action_status =
                                            LastActionStatus::Success("Successfully uninstalled mods.".to_string());
                                        },
                                        Err(e) => {
                                            self.last_action_status = LastActionStatus::Failure(
                                                format!("Failed to uninstall mods: {e}"),
                                            )
                                        }
                                    }
                                }
                            }
                        });

                        if ui
                            .button("Update cache")
                            .on_hover_text("Checks for updates for all mods and updates local cache.")
                            .clicked()
                        {
                            message::UpdateCache::send(self);
                        }
                    },
                );
                if self.integrate_rid.is_some() {
                    if ui.button("Cancel").clicked() {
                        self.integrate_rid.take().unwrap().handle.abort();
                    }
                    ui.spinner();
                }
                if self.update_rid.is_some() {
                    if ui.button("Cancel").clicked() {
                        self.update_rid.take().unwrap().handle.abort();
                    }
                    ui.spinner();
                }
                if ui.button("Lint mods").on_hover_text("Lint mods in the current profile.").clicked() {
                    let mut mods = Vec::new();
                    let active_profile = self.state.mod_data.active_profile.clone();
                    self.state
                        .mod_data
                        .for_each_enabled_mod(&active_profile, |mc| {
                            mods.push(mc.spec.clone());
                        });
                    self.lints_toggle_window = Some(WindowLintsToggle { mods });
                }
                if ui.button("⚙").on_hover_text("Open mint-notag settings.").clicked() {
                    self.settings_window = Some(WindowSettings::new(&self.state));
                }
                if ui.button("\u{2139}").on_hover_text("About mint-notag.").clicked() {
                    self.about_window = Some(WindowAbout::new());
                }
                if let Some(available_update) = &self.available_update {
                    if ui
                        // Dodger blue arrow
                        .button(egui::RichText::new("\u{21BB}").color(Color32::from_rgb(0, 143, 255)))
                        .on_hover_text(format!("Update available: {}.\nClick to open the download page.", available_update.tag_name))
                        .clicked() {
                            ui.ctx().output_mut(|o| {
                                o.open_url = Some(egui::output::OpenUrl {
                                    url: available_update.html_url.clone(),
                                    new_tab: true,
                                });
                            });
                        }
                }
                ui.with_layout(egui::Layout::left_to_right(Align::TOP), |ui| {
                    match &self.last_action_status {
                        LastActionStatus::Success(msg) => {
                            ui.label(
                                egui::RichText::new(" SUCCESS ")
                                    .color(Color32::BLACK)
                                    .background_color(Color32::LIGHT_GREEN),
                            );
                            ui.label(msg);
                        }
                        LastActionStatus::Failure(msg) => {
                            ui.label(
                                egui::RichText::new(" FAILED ")
                                    .color(Color32::BLACK)
                                    .background_color(Color32::LIGHT_RED),
                            );
                            ui.label(msg);
                        }
                        _ => {}
                    }
                });
            });
        });
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.set_enabled(
                self.integrate_rid.is_none()
                    && self.update_rid.is_none()
                    && self.lint_rid.is_none(),
            );

            // profile selection
            let buttons = |ui: &mut Ui, mod_data: &mut ModData| {
                if ui
                    .button("📋")
                    .on_hover_text_at_pointer("Copy mods URL in the current profile.")
                    .clicked()
                {
                    let mut mods = Vec::new();
                    let active_profile = mod_data.active_profile.clone();
                    mod_data.for_each_enabled_mod(&active_profile, |mc| {
                        mods.push(mc.clone());
                    });
                    let mods = Self::build_mod_string(&mods);
                    ui.output_mut(|o| o.copied_text = mods);
                }

                // TODO find better icon, flesh out multiple-view usage, fix GUI locking
                /*
                if ui
                    .button("pop out")
                    .on_hover_text_at_pointer("pop out")
                    .clicked()
                {
                    self.open_profiles.insert(mod_data.active_profile.clone());
                }
                */
            };

            if named_combobox::ui(
                ui,
                "profile",
                self.state.mod_data.deref_mut().deref_mut(),
                Some(buttons),
            ) {
                self.state.mod_data.save().unwrap();
            }

            ui.separator();

            ui.with_layout(egui::Layout::right_to_left(Align::TOP), |ui| {
                if self.resolve_mod_rid.is_some() {
                    ui.spinner();
                }
                ui.with_layout(ui.layout().with_main_justify(true), |ui| {
                    // define multiline layouter to be able to show multiple lines in a single line widget
                    let font_id = FontSelection::default().resolve(ui.style());
                    let text_color = ui.visuals().widgets.inactive.text_color();
                    let mut multiline_layouter = move |ui: &Ui, text: &str, wrap_width: f32| {
                        let layout_job = LayoutJob::simple(
                            text.to_string(),
                            font_id.clone(),
                            text_color,
                            wrap_width,
                        );
                        ui.fonts(|f| f.layout_job(layout_job))
                    };

                    let resolve = ui.add_enabled(
                        self.resolve_mod_rid.is_none(),
                        egui::TextEdit::singleline(&mut self.resolve_mod)
                            .layouter(&mut multiline_layouter)
                            .hint_text("Add mod..."),
                    );
                    if is_committed(&resolve) {
                        message::ResolveMods::send(self, ctx, self.parse_mods(), false);
                    }
                });
            });

            let profile = self.state.mod_data.active_profile.clone();
            self.ui_profile(ui, &profile);

            // TODO: actually implement mod groups.
            if let Some(search_string) = &mut self.search_string {
                let lower = search_string.to_lowercase();
                let any_matches = self.state.mod_data.any_mod(&profile, |mc, _| {
                    self.state
                        .store
                        .get_mod_info(&mc.spec)
                        .map(|i| i.name.to_lowercase().contains(&lower))
                        .unwrap_or(false)
                });

                let mut text_edit = egui::TextEdit::singleline(search_string);
                if !any_matches {
                    text_edit = text_edit.text_color(ui.visuals().error_fg_color);
                }
                let res = ui
                    .child_ui(ui.max_rect(), egui::Layout::bottom_up(Align::RIGHT))
                    .add(text_edit);
                if res.changed() {
                    self.scroll_to_match = true;
                }
                if res.lost_focus() {
                    self.search_string = None;
                    self.scroll_to_match = false;
                } else if !res.has_focus() {
                    res.request_focus();
                }
            }

            ctx.input(|i| {
                if !i.raw.dropped_files.is_empty()
                    && self.integrate_rid.is_none()
                    && self.update_rid.is_none()
                {
                    let mut mods = String::new();
                    for f in i
                        .raw
                        .dropped_files
                        .iter()
                        .filter_map(|f| f.path.as_ref().map(|p| p.to_string_lossy()))
                    {
                        mods.push_str(&f);
                        mods.push('\n');
                    }

                    self.resolve_mod = mods.trim().to_string();
                    message::ResolveMods::send(self, ctx, self.parse_mods(), false);
                }
                for e in &i.events {
                    match e {
                        egui::Event::Paste(s) => {
                            if self.integrate_rid.is_none()
                                && self.update_rid.is_none()
                                && self.lint_rid.is_none()
                                && ctx.memory(|m| m.focus().is_none())
                            {
                                self.resolve_mod = s.trim().to_string();
                                message::ResolveMods::send(self, ctx, self.parse_mods(), false);
                            }
                        }
                        egui::Event::Text(text) => {
                            if ctx.memory(|m| m.focus().is_none()) {
                                self.search_string = Some(text.to_string());
                                self.scroll_to_match = true;
                            }
                        }
                        egui::Event::Key { key, modifiers, pressed, .. } => {
                            if key == &egui::Key::Q && *pressed && modifiers.ctrl {
                                std::process::exit(0);
                            }
                        }
                        _ => {}
                    }
                }
            });
        });
    }
}

fn is_committed(res: &egui::Response) -> bool {
    res.lost_focus() && res.ctx.input(|i| i.key_pressed(egui::Key::Enter))
}

/// A custom popup which does not automatically close when clicked.
fn custom_popup_above_or_below_widget<R>(
    ui: &Ui,
    popup_id: egui::Id,
    widget_response: &egui::Response,
    above_or_below: egui::AboveOrBelow,
    add_contents: impl FnOnce(&mut Ui) -> R,
) -> Option<R> {
    if ui.memory(|mem| mem.is_popup_open(popup_id)) {
        let (pos, pivot) = match above_or_below {
            egui::AboveOrBelow::Above => (widget_response.rect.left_top(), Align2::LEFT_BOTTOM),
            egui::AboveOrBelow::Below => (widget_response.rect.left_bottom(), Align2::LEFT_TOP),
        };

        let inner = egui::Area::new(popup_id)
            .order(egui::Order::Foreground)
            .constrain(true)
            .fixed_pos(pos)
            .pivot(pivot)
            .show(ui.ctx(), |ui| {
                // Note: we use a separate clip-rect for this area, so the popup can be outside the parent.
                // See https://github.com/emilk/egui/issues/825
                let frame = egui::Frame::popup(ui.style());
                let frame_margin = frame.total_margin();
                frame
                    .show(ui, |ui| {
                        ui.with_layout(Layout::top_down_justified(Align::LEFT), |ui| {
                            ui.set_width(widget_response.rect.width() - frame_margin.sum().x);
                            add_contents(ui)
                        })
                        .inner
                    })
                    .inner
            })
            .inner;

        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            ui.memory_mut(|mem| mem.close_popup());
        }
        Some(inner)
    } else {
        None
    }
}

#[derive(Debug, serde::Deserialize)]
pub struct GitHubRelease {
    html_url: String,
    tag_name: String,
    body: String,
}

#[derive(Debug)]
pub enum SpecFetchProgress {
    Progress { progress: u64, size: u64 },
    Complete,
}

impl From<FetchProgress> for SpecFetchProgress {
    fn from(value: FetchProgress) -> Self {
        match value {
            FetchProgress::Progress { progress, size, .. } => Self::Progress { progress, size },
            FetchProgress::Complete { .. } => Self::Complete,
        }
    }
}
