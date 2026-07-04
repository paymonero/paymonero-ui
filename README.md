# paymonero-ui

Web UI for [paymonero-core](https://github.com/paymonero/paymonero-core). Provides a merchant dashboard and customer-facing payment pages.

Reads directly from the same PostgreSQL database as the core. No API calls at runtime.

## Pages

| Route | Description |
|---|---|
| `/admin` | Dashboard — invoice list with stats and filters |
| `/admin/invoice/:id` | Invoice detail — amounts, transactions, webhook status |
| `/pay/:id` | Customer payment page — QR code, address, countdown timer |
| `/pay/:id/status` | Minimal status page for post-payment redirect |

Payment pages auto-refresh every 15 seconds via `<meta http-equiv="refresh">` — no JavaScript required.

## Preview

![Dashboard](assets/dashboard.png)

## Requirements

- Rust (stable)
- PostgreSQL (same instance as paymonero-core)

## Quick Start

**1. Configure**

```bash
cp .env.example .env
```

```env
DATABASE_URL=postgresql://paymonero:changeme@127.0.0.1:5432/paymonero
PAYMONERO_API_URL=http://127.0.0.1:3000
PAYMONERO_API_KEY=your-api-key
UI_HOST=127.0.0.1
UI_PORT=8000
```

**2. Run**

```bash
cargo run --release
```

Open `http://localhost:8000/admin`.

## Production Deployment

```ini
# /etc/systemd/system/paymonero-ui.service
[Unit]
Description=paymonero-ui
After=network.target postgresql.service

[Service]
ExecStart=/usr/local/bin/paymonero-ui
EnvironmentFile=/etc/paymonero/.env.ui
Restart=always
RestartSec=5
User=paymonero

[Install]
WantedBy=multi-user.target
```

### nginx

The `/admin` route is unauthenticated by default. Protect it with nginx Basic Auth in production.

**Create password file:**

```bash
sudo apt install apache2-utils
sudo htpasswd -c /etc/nginx/.htpasswd admin
```

**nginx config:**

```nginx
server {
    listen 443 ssl;
    server_name pay.yourdomain.com;

    # Admin dashboard — password protected
    location /admin {
        auth_basic "Admin";
        auth_basic_user_file /etc/nginx/.htpasswd;
        proxy_pass http://127.0.0.1:8000;
        proxy_set_header X-Real-IP $remote_addr;
    }

    # Customer payment pages — public
    location / {
        proxy_pass http://127.0.0.1:8000;
        proxy_set_header X-Real-IP $remote_addr;
    }
}
```

## Notes

- UI and core must be on the same version — they share the database schema
- The UI has read-only access to the database; it does not write

## Donations

If you find this project useful, XMR donations are appreciated:

```
8BMaq1zdXL1JoaMBb39kEobUaXbybXa4bAGT2E8cfr9xREUu5kAUP5bVYkUcacui9PNJm2ejb8WSL7B3beKuHQuBQQJjzZa
```
