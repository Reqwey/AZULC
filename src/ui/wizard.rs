use crate::{
    app::{
        Launcher, Message,
        navigation::{NewInstanceTab, Route, VersionFilter, WizardStep},
    },
    domain::LoaderKind,
    services::minecraft::VersionEntry,
    theme,
};
use iced::widget::{Space, button, column, container, row, rule, scrollable, text, text_input};
use iced::{Alignment, Element, Fill, alignment, padding};

use super::{CONTENT_END_GAP, SCROLLBAR_GAP, media};

pub(crate) fn view(app: &Launcher) -> Element<'_, Message> {
    let mut tabs = row![].spacing(7);
    for tab in NewInstanceTab::ALL {
        tabs = tabs.push(
            button(text(tab.label().to_uppercase()).size(13))
                .on_press(Message::NewInstanceTabSelected(tab))
                .padding([11, 17])
                .style(if app.new_instance_tab == tab {
                    theme::primary_button
                } else {
                    theme::ghost_button
                }),
        );
    }
    let page_tabs = row![
        text("NEW INSTANCE")
            .font(theme::BODY_BOLD)
            .size(10)
            .color(theme::MUTED),
        Space::new().width(18),
        tabs,
        Space::new().width(Fill)
    ]
    .align_y(Alignment::Center);

    let page = match app.new_instance_tab {
        NewInstanceTab::Minecraft => minecraft_view(app),
        NewInstanceTab::Modpacks => super::modpacks::view(app),
    };

    column![page_tabs, page]
        .spacing(15)
        .width(Fill)
        .height(Fill)
        .into()
}

fn minecraft_view(app: &Launcher) -> Element<'_, Message> {
    let header = text("BUILD A NEW INSTANCE")
        .size(31)
        .align_y(Alignment::Center);

    let stepper = row![
        step(
            "01",
            minecraft_step_title(app),
            None,
            WizardStep::Version,
            app,
        ),
        container(rule::horizontal(2)).width(70),
        step(
            "02",
            app.wizard.loader.label().to_uppercase(),
            loader_step_version(app),
            WizardStep::Loader,
            app,
        ),
        container(rule::horizontal(2)).width(70),
        step("03", "BASIC DETAILS".into(), None, WizardStep::Details, app,),
    ]
    .spacing(12)
    .align_y(Alignment::Center);

    let content = match app.wizard_step {
        WizardStep::Version => version_step(app),
        WizardStep::Loader => loader_step(app),
        WizardStep::Details => details_step(app),
    };
    let can_advance = match app.wizard_step {
        WizardStep::Version => app.wizard.selected_version.is_some(),
        WizardStep::Loader | WizardStep::Details => {
            app.wizard.selected_version.is_some() && loader_selection_ready(app)
        }
    };
    let footer = row![
        Space::new().width(Fill),
        button(text("CANCEL").size(13))
            .on_press(Message::Navigate(if app.selected.is_some() {
                Route::Instances
            } else {
                Route::Home
            }))
            .padding([10, 15])
            .style(theme::nav_button),
        button(text("BACK").size(13))
            .on_press_maybe((app.wizard_step != WizardStep::Version).then_some(Message::WizardBack))
            .padding([10, 15])
            .style(theme::ghost_button),
        button(
            text(if app.wizard_step == WizardStep::Details {
                "CREATE & INSTALL  >"
            } else {
                "NEXT  >"
            })
            .size(14)
        )
        .on_press_maybe(can_advance.then_some(Message::WizardNext))
        .padding([11, 18])
        .style(theme::primary_button)
    ]
    .spacing(9)
    .align_y(Alignment::Center);

    column![header, stepper, content, footer]
        .spacing(17)
        .width(Fill)
        .height(Fill)
        .padding(padding::bottom(CONTENT_END_GAP))
        .into()
}

fn step<'a>(
    number: &'static str,
    title: String,
    subtitle: Option<String>,
    target: WizardStep,
    app: &'a Launcher,
) -> Element<'a, Message> {
    let current = app.wizard_step == target;
    let complete = match target {
        WizardStep::Version => app.wizard.selected_version.is_some() && !current,
        WizardStep::Loader => app.wizard_step == WizardStep::Details,
        WizardStep::Details => false,
    };
    let title_color = if current { theme::TEXT } else { theme::MUTED };
    let mut copy = column![
        text(title)
            .font(theme::BODY_BOLD)
            .size(11)
            .color(title_color)
    ]
    .spacing(1);
    if let Some(subtitle) = subtitle {
        copy = copy.push(
            text(subtitle)
                .font(theme::BODY_FONT)
                .size(9)
                .color(if current {
                    theme::LAVENDER_SOFT
                } else {
                    theme::MUTED
                }),
        );
    }

    button(
        row![
            container(text(if complete { "✓" } else { number }).size(16).color(
                if current || complete {
                    theme::LAVENDER_SOFT
                } else {
                    theme::MUTED
                }
            ))
            .width(38)
            .height(38)
            .align_x(alignment::Horizontal::Center)
            .align_y(alignment::Vertical::Center)
            .style(if current || complete {
                theme::pill
            } else {
                theme::inset
            }),
            copy
        ]
        .spacing(9)
        .align_y(Alignment::Center),
    )
    .on_press(Message::WizardStepSelected(target))
    .padding(3)
    .style(theme::nav_button)
    .into()
}

fn version_step(app: &Launcher) -> Element<'_, Message> {
    let mut filters = row![].spacing(8);
    for filter in VersionFilter::ALL {
        let count = app
            .versions
            .iter()
            .filter(|entry| filter.matches(entry))
            .count();
        let selected = app.version_filter == filter;
        let accent = theme::instance_color(filter.color());
        filters = filters.push(
            button(
                row![
                    text(if selected { "◆" } else { "◇" })
                        .size(15)
                        .color(accent),
                    text(format!("{}  {count}", filter.label())).size(13)
                ]
                .spacing(7)
                .align_y(Alignment::Center),
            )
            .on_press(Message::VersionFilterSelected(filter))
            .padding([9, 12])
            .style(move |theme, status| {
                theme::color_swatch_button(theme, status, accent, selected)
            }),
        );
    }
    let controls = row![
        filters,
        Space::new().width(Fill),
        text_input("Search versions…", &app.wizard.search)
            .on_input(Message::VersionSearchChanged)
            .padding(10)
            .size(13)
            .width(245)
            .style(theme::square_text_input)
    ]
    .align_y(Alignment::Center);

    let search = app.wizard.search.trim().to_ascii_lowercase();
    let mut list = column![].spacing(6);
    let mut visible = 0usize;
    for version in app.versions.iter().filter(|entry| {
        app.version_filter.matches(entry)
            && (search.is_empty() || entry.id.to_ascii_lowercase().contains(&search))
    }) {
        visible += 1;
        if visible > 300 {
            break;
        }
        let selected = app.wizard.selected_version.as_deref() == Some(version.id.as_str());
        let released = version
            .release_time
            .split('T')
            .next()
            .unwrap_or("unknown date");
        let accent = theme::instance_color(VersionFilter::for_version(version).color());
        list = list.push(
            button(
                row![
                    container(
                        text(if selected { "◆" } else { "◇" })
                            .size(20)
                            .color(accent)
                    )
                    .width(38)
                    .height(40)
                    .align_x(alignment::Horizontal::Center)
                    .align_y(alignment::Vertical::Center),
                    column![
                        row![
                            text(&version.id).size(17),
                            text(version_badge(version))
                                .font(theme::BODY_BOLD)
                                .size(9)
                                .color(accent)
                        ]
                        .spacing(9)
                        .align_y(Alignment::Center),
                        text(released)
                            .font(theme::BODY_FONT)
                            .size(10)
                            .color(theme::MUTED)
                    ]
                    .spacing(3),
                    Space::new().width(Fill),
                    text("WORLD  >").font(theme::BODY_BOLD).size(9)
                ]
                .spacing(9)
                .align_y(Alignment::Center),
            )
            .width(Fill)
            .padding(12)
            .on_press(Message::VersionPicked(version.id.clone()))
            .style(move |theme, status| {
                theme::color_swatch_button(theme, status, accent, selected)
            }),
        );
    }
    if visible == 0 {
        list = list.push(
            container(
                column![
                    text(if app.versions.is_empty() {
                        "SYNCING VERSION CATALOG…"
                    } else {
                        "NO MATCHING BUILDS"
                    })
                    .size(20)
                    .color(theme::MUTED),
                    text("Try another channel or search term.")
                        .font(theme::BODY_FONT)
                        .size(11)
                        .color(theme::MUTED)
                ]
                .spacing(5),
            )
            .width(Fill)
            .padding(24)
            .style(theme::inset),
        );
    }
    container(
        column![
            controls,
            rule::horizontal(1),
            scrollable(list)
                .width(Fill)
                .height(Fill)
                .spacing(SCROLLBAR_GAP)
                .style(theme::square_scrollable)
        ]
        .spacing(12),
    )
    .width(Fill)
    .height(Fill)
    .padding(16)
    .style(theme::panel)
    .into()
}

fn loader_step(app: &Launcher) -> Element<'_, Message> {
    let mut choices = column![].spacing(10);
    for loader in LoaderKind::ALL {
        let selected = app.wizard.loader == loader;
        let detail = loader_copy(loader);
        choices = choices.push(
            button(
                row![
                    container(media::loader_icon(loader, 32.0))
                        .width(42)
                        .height(42)
                        .align_x(alignment::Horizontal::Center)
                        .align_y(alignment::Vertical::Center),
                    column![
                        text(loader.label()).size(19),
                        text(detail)
                            .font(theme::BODY_FONT)
                            .size(10)
                            .color(if selected {
                                theme::CANVAS
                            } else {
                                theme::MUTED
                            })
                    ]
                    .spacing(3),
                    Space::new().width(Fill),
                    text(if selected { "SELECTED" } else { ">" })
                        .font(theme::BODY_BOLD)
                        .size(10)
                ]
                .spacing(12)
                .align_y(Alignment::Center),
            )
            .width(Fill)
            .padding(16)
            .on_press(Message::LoaderPicked(loader))
            .style(if selected {
                theme::primary_button
            } else {
                theme::ghost_button
            }),
        );
    }

    let provider = app.loader_catalog.provider.unwrap_or("WAITING FOR CATALOG");
    let catalog_header = row![
        column![
            text("LOADER BUILD")
                .font(theme::BODY_BOLD)
                .size(10)
                .color(theme::LAVENDER),
            text(app.wizard.loader.label()).size(28),
            text(format!(
                "Minecraft {}",
                app.wizard
                    .selected_version
                    .as_deref()
                    .unwrap_or("not selected")
            ))
            .font(theme::BODY_FONT)
            .size(11)
            .color(theme::MUTED)
        ]
        .spacing(3),
        Space::new().width(Fill),
        column![
            text(format!("{} BUILDS", app.loader_catalog.entries.len()))
                .font(theme::BODY_BOLD)
                .size(11)
                .color(theme::TEXT),
            text(provider)
                .font(theme::BODY_FONT)
                .size(9)
                .color(theme::MUTED)
        ]
        .spacing(2)
        .align_x(alignment::Horizontal::Right)
    ]
    .align_y(Alignment::Center);

    let catalog_body: Element<'_, Message> = if app.wizard.loader == LoaderKind::Vanilla {
        container(
            column![
                text("NO LOADER PACKAGE REQUIRED").size(19),
                text("The base Minecraft profile will be installed without a mod-loader layer.")
                    .font(theme::BODY_FONT)
                    .size(11)
                    .color(theme::MUTED)
            ]
            .spacing(5),
        )
        .width(Fill)
        .height(Fill)
        .padding(20)
        .style(theme::inset)
        .into()
    } else if app.loader_catalog.loading {
        container(
            column![
                text("FETCHING COMPATIBLE BUILDS…")
                    .size(19)
                    .color(theme::LAVENDER_SOFT),
                text("Reading the loader catalog for the selected Minecraft version.")
                    .font(theme::BODY_FONT)
                    .size(11)
                    .color(theme::MUTED)
            ]
            .spacing(5),
        )
        .width(Fill)
        .height(Fill)
        .padding(20)
        .style(theme::inset)
        .into()
    } else if let Some(error) = &app.loader_catalog.error {
        container(
            column![
                text("CATALOG REQUEST FAILED").size(19).color(theme::DANGER),
                text(error)
                    .font(theme::BODY_FONT)
                    .size(10)
                    .color(theme::TEXT),
                button(text("RETRY CATALOG").size(13))
                    .on_press(Message::RetryLoaderCatalog)
                    .padding([9, 13])
                    .style(theme::ghost_button)
            ]
            .spacing(10),
        )
        .width(Fill)
        .height(Fill)
        .padding(20)
        .style(theme::danger_panel)
        .into()
    } else {
        let mut builds = column![].spacing(6);
        for entry in &app.loader_catalog.entries {
            let selected = app.wizard.loader_version == entry.install_version;
            let metadata = if !entry.description.is_empty() {
                entry
                    .description
                    .split('T')
                    .next()
                    .unwrap_or(&entry.description)
                    .to_string()
            } else if let Some(branch) = &entry.branch {
                format!("branch {branch}")
            } else {
                "compatible build".to_string()
            };
            builds = builds.push(
                button(
                    row![
                        text(if selected { "●" } else { "○" })
                            .size(17)
                            .color(if selected {
                                theme::CANVAS
                            } else {
                                theme::MUTED
                            }),
                        column![
                            text(&entry.version).size(16),
                            text(metadata)
                                .font(theme::BODY_FONT)
                                .size(9)
                                .color(if selected {
                                    theme::CANVAS
                                } else {
                                    theme::MUTED
                                })
                        ]
                        .spacing(1),
                        Space::new().width(Fill),
                        text(if entry.stable { "STABLE" } else { "TEST" })
                            .font(theme::BODY_BOLD)
                            .size(9)
                            .color(if selected {
                                theme::CANVAS
                            } else if entry.stable {
                                theme::SUCCESS
                            } else {
                                theme::WARNING
                            })
                    ]
                    .spacing(10)
                    .align_y(Alignment::Center),
                )
                .width(Fill)
                .padding([10, 12])
                .on_press(Message::LoaderVersionPicked(entry.install_version.clone()))
                .style(if selected {
                    theme::primary_button
                } else {
                    theme::ghost_button
                }),
            );
        }
        if app.loader_catalog.entries.is_empty() {
            builds = builds.push(
                container(
                    column![
                        text("NO COMPATIBLE BUILDS").size(19),
                        text("This loader does not publish a build for the selected Minecraft version.")
                            .font(theme::BODY_FONT)
                            .size(11)
                            .color(theme::MUTED)
                    ]
                    .spacing(5),
                )
                .width(Fill)
                .padding(20)
                .style(theme::inset),
            );
        }
        scrollable(builds)
            .width(Fill)
            .height(Fill)
            .spacing(SCROLLBAR_GAP)
            .style(theme::square_scrollable)
            .into()
    };

    let version = container(column![catalog_header, catalog_body].spacing(12))
        .width(Fill)
        .height(Fill)
        .padding(22)
        .style(theme::panel);

    row![container(choices).width(340), version]
        .spacing(18)
        .height(Fill)
        .into()
}

fn details_step(app: &Launcher) -> Element<'_, Message> {
    let selected = app.wizard.selected_version.as_deref().unwrap_or("none");
    let form = container(
        column![
            field(
                "INSTANCE NAME",
                text_input("My Minecraft instance", &app.wizard.name)
                    .on_input(Message::WizardNameChanged)
                    .padding(12)
                    .size(14)
                    .style(theme::square_text_input)
            ),
            field(
                "DESCRIPTION",
                text_input("What is this workspace for?", &app.wizard.description)
                    .on_input(Message::WizardDescriptionChanged)
                    .padding(12)
                    .size(14)
                    .style(theme::square_text_input)
            ),
            field(
                "MARKER COLOR",
                media::instance_color_picker(app.wizard.color, Message::WizardColorPicked)
            ),
            container(
                column![
                    text("INSTANCE ISOLATION").font(theme::BODY_BOLD).size(10).color(theme::SUCCESS),
                    text("A dedicated game directory will be created under AZULC data. Shared libraries and assets remain deduplicated.")
                        .font(theme::BODY_FONT)
                        .size(10)
                        .color(theme::MUTED)
                ]
                .spacing(5)
            )
            .width(Fill)
            .padding(14)
            .style(theme::inset)
        ]
        .spacing(15),
    )
    .width(Fill)
    .height(Fill)
    .padding(22)
    .style(theme::panel);

    let summary = container(
        column![
            text("BUILD SUMMARY")
                .font(theme::BODY_BOLD)
                .size(10)
                .color(theme::LAVENDER),
            media::instance_marker(app.wizard.color, 47),
            text(&app.wizard.name).size(25),
            summary_line("MINECRAFT", selected.to_string()),
            summary_line("LOADER", app.wizard.loader.label().to_string()),
            summary_line(
                "LOADER BUILD",
                if app.wizard.loader_version.trim().is_empty() {
                    "AUTO".into()
                } else {
                    app.wizard.loader_version.clone()
                }
            ),
            summary_line("SOURCE", app.persisted.settings.download.source.to_string()),
            summary_line(
                "WORKERS",
                app.persisted.settings.download.concurrency.to_string()
            )
        ]
        .spacing(12)
        .align_x(alignment::Horizontal::Center),
    )
    .width(330)
    .height(Fill)
    .padding(22)
    .style(theme::selected_card);

    row![form, summary].spacing(18).height(Fill).into()
}

fn field<'a>(label: &'a str, control: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    column![
        text(label)
            .font(theme::BODY_BOLD)
            .size(10)
            .color(theme::MUTED),
        control.into()
    ]
    .spacing(6)
    .into()
}

fn summary_line<'a>(label: &'a str, value: String) -> Element<'a, Message> {
    row![
        text(label)
            .font(theme::BODY_BOLD)
            .size(9)
            .color(theme::MUTED),
        Space::new().width(Fill),
        text(value)
            .font(theme::BODY_BOLD)
            .size(10)
            .color(theme::LAVENDER_SOFT)
    ]
    .width(Fill)
    .into()
}

fn minecraft_step_title(app: &Launcher) -> String {
    app.wizard
        .selected_version
        .clone()
        .unwrap_or_else(|| "MINECRAFT VERSION".into())
}

fn loader_step_version(app: &Launcher) -> Option<String> {
    if app.wizard.loader == LoaderKind::Vanilla {
        return Some("NO LOADER".into());
    }

    if app.wizard.loader_version.trim().is_empty() {
        return None;
    }

    Some(
        app.loader_catalog
            .entries
            .iter()
            .find(|entry| entry.install_version == app.wizard.loader_version)
            .map(|entry| entry.version.clone())
            .unwrap_or_else(|| app.wizard.loader_version.clone()),
    )
}

fn loader_selection_ready(app: &Launcher) -> bool {
    app.wizard.loader == LoaderKind::Vanilla || !app.wizard.loader_version.trim().is_empty()
}

fn version_badge(entry: &VersionEntry) -> &'static str {
    if entry.release_time.contains("04-01") {
        "APRIL"
    } else {
        match entry.kind.as_str() {
            "release" => "RELEASE",
            "snapshot" => "SNAPSHOT",
            _ => "LEGACY",
        }
    }
}

fn loader_copy(loader: LoaderKind) -> &'static str {
    match loader {
        LoaderKind::Vanilla => "Pure Minecraft / no loader",
        LoaderKind::Fabric => "Lightweight and fast modding",
        LoaderKind::Forge => "Classic mod ecosystem",
        LoaderKind::NeoForge => "Modern Forge-family platform",
    }
}
