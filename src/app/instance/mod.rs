//! Installed-instance mutation, deletion, and content management.

mod content;

pub(crate) use content::ResourceBrowserState;

use super::{Launcher, Message};
use crate::domain::Instance;
use iced::Task;
use uuid::Uuid;

impl Launcher {
    pub(super) fn edit_instance(&mut self, id: Uuid, edit: impl FnOnce(&mut Instance)) {
        if !matches!(self.route, super::navigation::Route::Instance { id: route_id, .. } if route_id == id)
        {
            return;
        }
        if let Some(instance) = self.instance_mut(id) {
            edit(instance);
            self.save();
        }
    }

    pub(super) fn delete_instance(&self, id: Uuid) -> Task<Message> {
        let path = self.paths.instance_dir(id);
        Task::perform(
            async move {
                tokio::fs::remove_dir_all(path)
                    .await
                    .or_else(|error| {
                        if error.kind() == std::io::ErrorKind::NotFound {
                            Ok(())
                        } else {
                            Err(error)
                        }
                    })
                    .map_err(|error| error.to_string())
            },
            move |result| Message::Deleted(id, result),
        )
    }
}
