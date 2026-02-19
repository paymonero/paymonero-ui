use chrono::{DateTime, Utc};
use sqlx::PgPool;

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct InvoiceRow {
    pub id: String,
    pub amount_atomic: i64,
    pub amount_received: i64,
    pub address: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub paid_at: Option<DateTime<Utc>>,
    pub txid: Option<String>,
    pub confirmations: i64,
    pub description: Option<String>,
    pub callback_url: Option<String>,
    pub callback_delivered: bool,
    pub callback_attempts: i32,
    pub overpayment_atomic: i64,
    pub external_id: Option<String>,
}

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct PaymentRow {
    pub id: i32,
    pub invoice_id: String,
    pub txid: String,
    pub amount_atomic: i64,
    pub confirmations: i64,
    pub height: i64,
    pub detected_at: DateTime<Utc>,
}

#[derive(Debug, Default)]
pub struct DashboardStats {
    pub total_invoices: i64,
    pub paid_count: i64,
    pub pending_count: i64,
    pub partial_count: i64,
    pub expired_count: i64,
    pub total_received_atomic: i64,
    pub total_expected_atomic: i64,
}

impl DashboardStats {
    pub async fn load(pool: &PgPool) -> Result<Self, sqlx::Error> {
        let total: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM invoices")
            .fetch_one(pool)
            .await?;

        let paid: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM invoices WHERE status = 'paid'")
            .fetch_one(pool)
            .await?;

        let pending: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM invoices WHERE status = 'pending'")
                .fetch_one(pool)
                .await?;

        let partial: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM invoices WHERE status = 'partial'")
                .fetch_one(pool)
                .await?;

        let expired: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM invoices WHERE status = 'expired'")
                .fetch_one(pool)
                .await?;

        let received: (Option<i64>,) = sqlx::query_as(
            "SELECT CAST(COALESCE(SUM(amount_received), 0) AS BIGINT) FROM invoices WHERE status = 'paid'",
        )
        .fetch_one(pool)
        .await?;

        let expected: (Option<i64>,) =
            sqlx::query_as("SELECT CAST(COALESCE(SUM(amount_atomic), 0) AS BIGINT) FROM invoices WHERE status = 'paid'")
                .fetch_one(pool)
                .await?;

        Ok(Self {
            total_invoices: total.0,
            paid_count: paid.0,
            pending_count: pending.0,
            partial_count: partial.0,
            expired_count: expired.0,
            total_received_atomic: received.0.unwrap_or(0),
            total_expected_atomic: expected.0.unwrap_or(0),
        })
    }

    pub fn total_received_xmr(&self) -> String {
        format_xmr(self.total_received_atomic)
    }

    #[allow(dead_code)]
    pub fn total_expected_xmr(&self) -> String {
        format_xmr(self.total_expected_atomic)
    }
}

pub fn format_xmr(atomic: i64) -> String {
    let a = atomic.unsigned_abs();
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
