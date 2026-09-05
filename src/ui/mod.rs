mod accounts;
pub(crate) mod brand;
mod components;
mod home;
mod instance;
mod modpacks;
mod overlays;
mod resource_browser;
mod settings;
mod shell;
mod wizard;

use crate::app::{Launcher, Message, navigation::Route};
use iced::Element;

impl Launcher {
    pub fn view(&self) -> Element<'_, Message> {
        let page = match self.route {
            Route::Home => home::view(self),
            Route::Instance { id, tab } => instance::view(self, id, tab),
            Route::Installation(id) => instance::installation_view(self, id),
            Route::NewInstance => wizard::view(self),
            Route::Accounts => accounts::view(self),
            Route::Settings => settings::view(self),
        };

        overlays::view(self, page)
    }
}
