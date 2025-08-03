#![feature(duration_constructors, duration_constructors_lite)]
use std::{net::SocketAddr, str::FromStr};

use askama::Template;
use axum::{
    extract::{Json, Path},
    http::StatusCode,
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
    Router,
};
use init::initialize;
use tokio::fs;
use tower_http::{services::ServeDir, trace::TraceLayer};

use crate::{event_db::FindEventError, templates::ManagePageJson};

pub mod cli;
pub mod event_db;
pub mod init;
pub mod templates;

const MODULE_NAME: &str = "invite";
const CONTENT_DIR: &str = "content";

#[tokio::main]
async fn main() {
    let (args, _logger_handle) = initialize();
    log::debug!("Completed initialization");

    let addr = SocketAddr::new(args.web_addr, args.http_port);
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();

    event_db::setup_test().await;

    // if defined, register with the slot server
    if let Some(slot_port) = args.slot_port {
        let module_name =
            slot_client::protocol::ValidName::from_str(MODULE_NAME)
                .expect("The constant module name is valid");

        slot_client::client_impl::run_client(
            slot_port,
            module_name,
            listener.local_addr().expect("HTTP socket is bound").port(),
        );
    }

    // set up webserver
    let routes = Router::new()
        .route("/invite/index", get(index_page))
        .nest_service("/invite/content", ServeDir::new(CONTENT_DIR))
        .layer(TraceLayer::new_for_http())
        // invite module specific routes
        .route("/invite/organize", get(create_new_event))
        .route("/invite/manage/{ev_id}", get(manage_event))
        .route("/invite/update/{ev_id}", post(update_event))
        .route("/invite/add/{ev_id}", post(add_attendee))
        .route("/invite/remove/{at_id}", post(remove_attendee))
        .route("/invite/attend/{at_id}", get(view_invitation))
        .route("/invite/accept/{at_id}", get(accept_invitation))
        .route("/invite/withdraw/{at_id}", get(withdraw_invitation))
        .route("/invite/thanks/{at_id}", get(view_event))
        .route("/invite", get(index_page));
    axum::serve(listener, routes).await.unwrap();
}

async fn create_new_event() -> Response {
    let ev_id = match event_db::create_event().await {
        Ok(v) => v,
        Err(e) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, e).into_response();
        }
    };
    let encoded_id = base62::encode(ev_id);
    let redirect_url = format!("/invite/manage/{encoded_id}");
    Redirect::to(&redirect_url).into_response()
}

async fn manage_event(Path(id): Path<String>) -> Response {
    // find event
    let ev_id = match base62::decode(&id) {
        Ok(v) => v,
        Err(_) => {
            return (StatusCode::NOT_FOUND, "Event does not exist")
                .into_response();
        }
    };
    let event = match event_db::find_event_by_id(ev_id as u64).await {
        Ok(v) => v,
        Err(FindEventError::Database(e)) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, e).into_response();
        }
        Err(FindEventError::NotFound(e)) => {
            return (StatusCode::NOT_FOUND, e).into_response();
        }
    };

    // render response
    let event_name = event.name.unwrap_or("Untitled Event".to_string());
    let Ok(template) = templates::ManagePage {
        event_name: &event_name,
        attendees: event
            .attendees
            .into_iter()
            .map(templates::Attendee::from)
            .collect(),
        update_link: &format!("/invite/update/{}", id),
        add_link: &format!("/invite/add/{}", id),
    }
    .render() else {
        return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to render page")
            .into_response();
    };
    Html(template).into_response()
}

async fn update_event(
    Path(id): Path<String>,
    Json(form): Json<ManagePageJson>,
) -> Redirect {
    let redirect = Redirect::to(&format!("/invite/manage/{id}"));
    // find event
    let ev_id = match base62::decode(&id) {
        Ok(v) => v,
        Err(_) => {
            log::error!("Event does not exist");
            return redirect;
        }
    };

    match event_db::update_event(ev_id as u64, form).await {
        Ok(_) => {}
        Err(event_db::FindEventError::Database(e)) => {
            log::error!("{e}");
        }
        Err(_) => {}
    }

    redirect
}

async fn add_attendee(Path(id): Path<String>) -> Redirect {
    let redirect = Redirect::to(&format!("/invite/manage/{id}"));
    // find event
    let ev_id = match base62::decode(&id) {
        Ok(v) => v,
        Err(_) => {
            log::error!("Event does not exist");
            return redirect;
        }
    };

    match event_db::add_attendee(ev_id as u64).await {
        Ok(_) => {}
        Err(event_db::FindEventError::Database(e)) => {
            log::error!("{e}");
        }
        Err(_) => {}
    }

    redirect
}

async fn remove_attendee(Path(id): Path<String>) -> Redirect {
    let redirect = Redirect::to(&format!("/invite/manage/{id}"));
    // find event
    let at_id = match base62::decode(&id) {
        Ok(v) => v,
        Err(_) => {
            log::error!("Event does not exist");
            return redirect;
        }
    };

    match event_db::remove_attendee(at_id as u64).await {
        Ok(_) => {}
        Err(event_db::FindEventError::Database(e)) => {
            log::error!("{e}");
        }
        Err(_) => {}
    }

    redirect
}

async fn view_invitation(Path(id): Path<String>) -> Response {
    // find event
    let at_id = match base62::decode(&id) {
        Ok(v) => v,
        Err(_) => {
            return (StatusCode::NOT_FOUND, "Event does not exist")
                .into_response();
        }
    };
    let (event, attendee) =
        match event_db::find_event_by_attendee(at_id as u64).await {
            Ok(v) => v,
            Err(FindEventError::Database(e)) => {
                return (StatusCode::INTERNAL_SERVER_ERROR, e).into_response();
            }
            Err(FindEventError::NotFound(e)) => {
                return (StatusCode::NOT_FOUND, e).into_response();
            }
        };
    let event_name = event.name.unwrap_or("Untitled Event".to_string());

    // if accepted, show withdraw page instead
    if attendee.has_accepted {
        let Ok(template) = templates::WithdrawPage {
            event_name: &event_name,
            withdraw_link: &format!("/invite/withdraw/{}", id),
        }
        .render() else {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to render page",
            )
                .into_response();
        };
        return Html(template).into_response();
    }

    // render template
    let mut ctx = tera::Context::new();
    ctx.insert("event_name", &event_name);
    ctx.insert("attendee_name", &attendee.name);
    ctx.insert("accept_link", &format!("/invite/accept/{}", id));
    let Ok(page) = tera::Tera::one_off(&attendee.custom_html, &ctx, true)
    else {
        // TODO: replace with a default page
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to render the custom invitation. Please contact the event \
             organizer and let them know.",
        )
            .into_response();
    };
    Html(page).into_response()
}

async fn accept_invitation(Path(id): Path<String>) -> Response {
    // find event
    let at_id = match base62::decode(&id) {
        Ok(v) => v,
        Err(_) => {
            return (StatusCode::NOT_FOUND, "Event does not exist")
                .into_response();
        }
    };
    match event_db::set_accepted(at_id as u64, true).await {
        Ok(_) => {}
        Err(FindEventError::Database(e)) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, e).into_response();
        }
        Err(FindEventError::NotFound(e)) => {
            return (StatusCode::NOT_FOUND, e).into_response();
        }
    }

    // redirect
    Redirect::to(&format!("/invite/thanks/{}", id)).into_response()
}

async fn withdraw_invitation(Path(id): Path<String>) -> Response {
    // find event
    let at_id = match base62::decode(&id) {
        Ok(v) => v,
        Err(_) => {
            return (StatusCode::NOT_FOUND, "Event does not exist")
                .into_response();
        }
    };
    match event_db::set_accepted(at_id as u64, false).await {
        Ok(_) => {}
        Err(FindEventError::Database(e)) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, e).into_response();
        }
        Err(FindEventError::NotFound(e)) => {
            return (StatusCode::NOT_FOUND, e).into_response();
        }
    }

    // redirect
    Redirect::to(&format!("/invite/attend/{}", id)).into_response()
}

async fn view_event(Path(id): Path<String>) -> Response {
    // find event
    let at_id = match base62::decode(&id) {
        Ok(v) => v,
        Err(_) => {
            return (StatusCode::NOT_FOUND, "Event does not exist")
                .into_response();
        }
    };
    let (event, _attendee) =
        match event_db::find_event_by_attendee(at_id as u64).await {
            Ok(v) => v,
            Err(FindEventError::Database(e)) => {
                return (StatusCode::INTERNAL_SERVER_ERROR, e).into_response();
            }
            Err(FindEventError::NotFound(e)) => {
                return (StatusCode::NOT_FOUND, e).into_response();
            }
        };

    // render response
    let event_name = event.name.unwrap_or("Untitled Event".to_string());
    let Ok(template) = templates::ThanksPage {
        event_name: &event_name,
    }
    .render() else {
        return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to render page")
            .into_response();
    };
    Html(template).into_response()
}

async fn index_page() -> Html<Vec<u8>> {
    Html(
        fs::read(std::path::Path::new(CONTENT_DIR).join("pages/index.html"))
            .await
            .expect("index.html exists"),
    )
}
