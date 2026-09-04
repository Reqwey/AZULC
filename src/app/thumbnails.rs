//! Remote catalog and local content thumbnail caching.

use super::{Launcher, Message};
use crate::services::{catalog::CatalogProject, content::ContentEntry, thumbnail};
use iced::{Task, widget::image};
use std::path::Path;

impl Launcher {
    pub(super) fn load_thumbnails(&mut self, urls: Vec<String>) -> Task<Message> {
        let missing = urls
            .into_iter()
            .filter(|url| !url.trim().is_empty())
            .filter(|url| !self.thumbnails.contains_key(url))
            .filter(|url| self.requested_thumbnails.insert(url.clone()))
            .collect::<Vec<_>>();
        if missing.is_empty() {
            Task::none()
        } else {
            Task::perform(
                thumbnail::fetch_remote_batch(missing),
                Message::ThumbnailsLoaded,
            )
        }
    }

    pub(crate) fn catalog_thumbnail(&self, project: &CatalogProject) -> Option<&image::Handle> {
        project
            .icon_url
            .as_ref()
            .and_then(|url| self.thumbnails.get(url))
    }

    pub(crate) fn content_thumbnail(&self, entry: &ContentEntry) -> Option<&image::Handle> {
        self.thumbnails.get(&local_thumbnail_key(&entry.path))
    }
}

pub(super) fn local_thumbnail_key(path: &Path) -> String {
    format!("local:{}", path.to_string_lossy())
}
