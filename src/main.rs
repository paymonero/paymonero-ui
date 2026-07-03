use askama::Template;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{Html, IntoResponse, Redirect, Response},
    routing::get,
    Router,
};
use sqlx::PgPool;
use std::sync::Arc;
use tower_http::services::ServeDir;

mod models;
mod qr;

use models::*;

// ── App State ──────────────────────────────────────────────────────

struct AppState {
    pool: PgPool,
    #[allow(dead_code)]
    api_url: String,
    #[allow(dead_code)]
    api_key: String,
}

// ── Templates ──────────────────────────────────────────────────────

#[derive(Template)]
#[template(path = "dashboard.html")]
struct DashboardTemplate {
    invoices: Vec<InvoiceRow>,
    stats: DashboardStats,
    filter: String,
    page: i64,
    total_pages: i64,
    pagination_html: String,
}

#[derive(Template)]
#[template(path = "invoice_detail.html")]
struct InvoiceDetailTemplate {
    invoice: InvoiceRow,
    payments: Vec<PaymentRow>,
    shortfall_xmr: String,
    overpayment_xmr: String,
}

#[derive(Template)]
#[template(path = "pay.html")]
struct PayTemplate {
    invoice: InvoiceRow,
    qr_svg: String,
    monero_uri: String,
    remaining_xmr: String,
    remaining_atomic: i64,
    time_left_secs: i64,
}

#[derive(Template)]
#[template(path = "pay_status.html")]
struct PayStatusTemplate {
    invoice: InvoiceRow,
}

// ── Error handling ─────────────────────────────────────────────────

struct AppError(String);

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Html(format!(
                "<h1>Error</h1><pre>{}</pre><p><a href=\"/admin\">Back to dashboard</a></p>",
                self.0
            )),
        )
            .into_response()
    }
}

// ── Filters for Askama ─────────────────────────────────────────────

mod filters {
    pub fn format_xmr(atomic: &i64) -> askama::Result<String> {
        let a = *atomic as u64;
        let whole = a / 1_000_000_000_000;
        let frac = a % 1_000_000_000_000;
        Ok(format!("{}.{:012}", whole, frac))
    }

    pub fn format_xmr_short(atomic: &i64) -> askama::Result<String> {
        let a = *atomic as f64 / 1_000_000_000_000.0;
        Ok(format!("{:.4}", a))
    }

    pub fn timeago(dt: &chrono::DateTime<chrono::Utc>) -> askama::Result<String> {
        let now = chrono::Utc::now();
        let diff = now - *dt;

        if diff.num_seconds() < 60 {
            Ok("just now".to_string())
        } else if diff.num_minutes() < 60 {
            Ok(format!("{}m ago", diff.num_minutes()))
        } else if diff.num_hours() < 24 {
            Ok(format!("{}h ago", diff.num_hours()))
        } else {
            Ok(format!("{}d ago", diff.num_days()))
        }
    }

    pub fn status_class(status: &str) -> askama::Result<String> {
        Ok(match status {
            "paid" => "status-paid",
            "pending" => "status-pending",
            "partial" => "status-partial",
            "new" => "status-new",
            "expired" => "status-expired",
            "cancelled" => "status-cancelled",
            _ => "status-unknown",
        }
        .to_string())
    }

    #[allow(dead_code)]
    pub fn truncate_addr(addr: &str) -> askama::Result<String> {
        if addr.len() > 20 {
            Ok(format!("{}…{}", &addr[..8], &addr[addr.len() - 8..]))
        } else {
            Ok(addr.to_string())
        }
    }
}

// ── Query params ───────────────────────────────────────────────────

#[derive(serde::Deserialize)]
struct ListParams {
    #[serde(default)]
    status: Option<String>,
    #[serde(default = "default_page")]
    page: i64,
}

fn default_page() -> i64 {
    1
}

// ── Routes ─────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    dotenv::dotenv().ok();
    tracing_subscriber::fmt::init();

    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL required");
    let api_url =
        std::env::var("PAYMONERO_API_URL").unwrap_or_else(|_| "http://127.0.0.1:3000".to_string());
    let api_key = std::env::var("PAYMONERO_API_KEY").unwrap_or_else(|_| "none".to_string());
    let host = std::env::var("UI_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let port = std::env::var("UI_PORT").unwrap_or_else(|_| "8080".to_string());

    let pool = PgPool::connect(&database_url)
        .await
        .expect("Failed to connect to database");

    let state = Arc::new(AppState {
        pool,
        api_url,
        api_key,
    });

    let app = Router::new()
        .route("/admin", get(dashboard))
        .route("/admin/invoice/:id", get(invoice_detail))
        .route("/pay/:id", get(pay_page))
        .route("/pay/:id/status", get(pay_status))
        .nest_service("/static", ServeDir::new("static"))
        .route("/", get(|| async { Redirect::to("/admin") }))
        .with_state(state);

    let addr = format!("{}:{}", host, port);
    tracing::info!("PayMonero UI listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

// ── Admin Dashboard ────────────────────────────────────────────────

const PAGE_SIZE: i64 = 25;

async fn dashboard(
    State(state): State<Arc<AppState>>,
    Query(params): Query<ListParams>,
) -> Result<Html<String>, AppError> {
    let page = params.page.max(1);
    let offset = (page - 1) * PAGE_SIZE;
    let filter = params.status.clone().unwrap_or_default();

    let (invoices, total) = if filter.is_empty() {
        let rows = sqlx::query_as::<_, InvoiceRow>(
            "SELECT * FROM invoices ORDER BY created_at DESC LIMIT $1 OFFSET $2",
        )
        .bind(PAGE_SIZE)
        .bind(offset)
        .fetch_all(&state.pool)
        .await
        .map_err(|e| AppError(e.to_string()))?;

        let total: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM invoices")
            .fetch_one(&state.pool)
            .await
            .map_err(|e| AppError(e.to_string()))?;

        (rows, total.0)
    } else {
        let rows = sqlx::query_as::<_, InvoiceRow>(
            "SELECT * FROM invoices WHERE status = $1 ORDER BY created_at DESC LIMIT $2 OFFSET $3",
        )
        .bind(&filter)
        .bind(PAGE_SIZE)
        .bind(offset)
        .fetch_all(&state.pool)
        .await
        .map_err(|e| AppError(e.to_string()))?;

        let total: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM invoices WHERE status = $1")
            .bind(&filter)
            .fetch_one(&state.pool)
            .await
            .map_err(|e| AppError(e.to_string()))?;

        (rows, total.0)
    };

    let total_pages = (total + PAGE_SIZE - 1) / PAGE_SIZE;
    let pagination_html = build_pagination(page, total_pages, &filter);
    let stats = DashboardStats::load(&state.pool)
        .await
        .map_err(|e| AppError(e.to_string()))?;

    let tmpl = DashboardTemplate {
        invoices,
        stats,
        filter,
        page,
        total_pages,
        pagination_html,
    };

    Ok(Html(tmpl.render().map_err(|e| AppError(e.to_string()))?))
}

// ── Invoice Detail ─────────────────────────────────────────────────

async fn invoice_detail(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Html<String>, AppError> {
    let invoice = sqlx::query_as::<_, InvoiceRow>("SELECT * FROM invoices WHERE id = $1")
        .bind(&id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|e| AppError(e.to_string()))?
        .ok_or_else(|| AppError("Invoice not found".to_string()))?;

    let payments = sqlx::query_as::<_, PaymentRow>(
        "SELECT * FROM invoice_payments WHERE invoice_id = $1 ORDER BY height ASC",
    )
    .bind(&id)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| AppError(e.to_string()))?;

    let required = invoice.amount_atomic;
    let received = invoice.amount_received;
    let shortfall = (required - received).max(0);
    let overpayment = (received - required).max(0);

    let tmpl = InvoiceDetailTemplate {
        invoice,
        payments,
        shortfall_xmr: format_xmr_display(shortfall),
        overpayment_xmr: format_xmr_display(overpayment),
    };

    Ok(Html(tmpl.render().map_err(|e| AppError(e.to_string()))?))
}

// ── Public Payment Page ────────────────────────────────────────────

async fn pay_page(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Html<String>, AppError> {
    let invoice = sqlx::query_as::<_, InvoiceRow>("SELECT * FROM invoices WHERE id = $1")
        .bind(&id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|e| AppError(e.to_string()))?
        .ok_or_else(|| AppError("Invoice not found".to_string()))?;

    let remaining = (invoice.amount_atomic - invoice.amount_received).max(0);
    let remaining_xmr = format_xmr_display(remaining);

    let monero_uri = format!(
        "monero:{}?tx_amount={}",
        invoice.address,
        format_xmr_display(remaining)
    );

    let qr_svg = qr::generate_svg(&monero_uri);

    let time_left = (invoice.expires_at - chrono::Utc::now())
        .num_seconds()
        .max(0);

    let tmpl = PayTemplate {
        invoice,
        qr_svg,
        monero_uri,
        remaining_xmr,
        remaining_atomic: remaining,
        time_left_secs: time_left,
    };

    Ok(Html(tmpl.render().map_err(|e| AppError(e.to_string()))?))
}

async fn pay_status(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Html<String>, AppError> {
    let invoice = sqlx::query_as::<_, InvoiceRow>("SELECT * FROM invoices WHERE id = $1")
        .bind(&id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|e| AppError(e.to_string()))?
        .ok_or_else(|| AppError("Invoice not found".to_string()))?;

    let tmpl = PayStatusTemplate { invoice };
    Ok(Html(tmpl.render().map_err(|e| AppError(e.to_string()))?))
}

fn format_xmr_display(atomic: i64) -> String {
    let a = atomic as u64;
    let whole = a / 1_000_000_000_000;
    let frac = a % 1_000_000_000_000;
    let frac_str = format!("{:012}", frac);
    let trimmed = frac_str.trim_end_matches('0');
    if trimmed.is_empty() {
        format!("{}.0", whole)
    } else {
        format!("{}.{}", whole, trimmed)
    }
}

fn build_pagination(page: i64, total_pages: i64, filter: &str) -> String {
    if total_pages <= 1 {
        return String::new();
    }

    let filter_param = if filter.is_empty() {
        String::new()
    } else {
        format!("&status={}", filter)
    };

    let url = |p: i64| format!("/admin?page={}{}", p, filter_param);

    let mut html = String::from("<div class=\"pagination\">");

    if page > 1 {
        html.push_str(&format!("<a href=\"{}\">←</a>", url(page - 1)));
    }
    if page > 3 {
        html.push_str(&format!("<a href=\"{}\">1</a>", url(1)));
        if page > 4 {
            html.push_str("<span>…</span>");
        }
    }
    for p in ((page - 2).max(1))..=((page + 2).min(total_pages)) {
        if p == page {
            html.push_str(&format!("<span class=\"current\">{}</span>", p));
        } else {
            html.push_str(&format!("<a href=\"{}\">{}</a>", url(p), p));
        }
    }
    if page < total_pages - 2 {
        if page < total_pages - 3 {
            html.push_str("<span>…</span>");
        }
        html.push_str(&format!(
            "<a href=\"{}\">{}</a>",
            url(total_pages),
            total_pages
        ));
    }
    if page < total_pages {
        html.push_str(&format!("<a href=\"{}\">→</a>", url(page + 1)));
    }

    html.push_str("</div>");
    html
}
