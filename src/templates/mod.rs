use std::collections::HashMap;

use askama::Template;
use serde::Deserialize;

pub struct Attendee {
    pub name: String,
    pub custom_html: String,
    pub has_accepted: bool,
    pub id: String,
    pub invite_link: String,
    pub remove_link: String,
}

impl From<crate::event_db::Attendee> for Attendee {
    fn from(value: crate::event_db::Attendee) -> Self {
        let encoded_id = base62::encode(value.id);
        Self {
            name: value.name,
            custom_html: value.custom_html,
            has_accepted: value.has_accepted,
            id: encoded_id.clone(),
            // full link since this will be copied by event organizer
            invite_link: format!(
                "https://blacepos.xyz/invite/attend/{}",
                encoded_id
            ),
            remove_link: format!("/invite/remove/{}", encoded_id),
        }
    }
}

#[derive(Template)]
#[template(path = "manage_event.html")]
pub struct ManagePage<'a> {
    pub event_name: &'a str,
    pub attendees: Vec<Attendee>,
    pub update_link: &'a str,
    pub add_link: &'a str,
}

#[derive(Deserialize, Debug)]
pub struct ManagePageJson {
    pub event_name: String,
    pub attendee_data: HashMap<String, ManagePageAttendeeJson>,
}

#[derive(Deserialize, Debug)]
pub struct ManagePageAttendeeJson {
    pub name: String,
    pub custom_html: String,
}

#[derive(Template)]
#[template(path = "thanks.html")]
pub struct ThanksPage<'a> {
    pub event_name: &'a str,
}

#[derive(Template)]
#[template(path = "withdraw_invitation.html")]
pub struct WithdrawPage<'a> {
    pub event_name: &'a str,
    pub withdraw_link: &'a str,
}
