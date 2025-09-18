# P2P Chat Application

A secure, real-time peer-to-peer (P2P) one-on-one chat app using WebRTC for direct connections and a Rust signaling server for initial setup.

## Overview

- **Backend**: Signaling server built with Axum (Rust) handling WebSocket signaling for SDP/ICE exchange. Supports JWT auth, in-memory room management, input validation, rate limiting, and CORS.
- **Frontend**: Leptos (Rust to WASM) web app for login, chat UI, and WebRTC data channels.
- **Communication**: P2P via WebRTC data channels for encrypted text messages; signaling server does not relay messages.
- **Security**: End-to-end encryption via WebRTC (DTLS), WSS for signaling (TLS in production), input validation, JWT auth.
- **Features**: Reconnection logic, message queuing, connection status feedback, cross-browser compatibility.

## Project Structure

```
.
├── Cargo.toml          # Workspace config
├── backend/            # Signaling server
│   ├── Cargo.toml
│   └── src/
│       └── main.rs
├── frontend/           # Leptos web app
│   ├── Cargo.toml
│   ├── Trunk.toml
│   ├── index.html
│   └── src/
│       └── lib.rs
├── LICENSE
└── README.md
```

## Prerequisites

- Rust (1.75+): https://rustup.rs/
- Trunk: `cargo install trunk`
- For frontend build: WASM target `rustup target add wasm32-unknown-unknown`

## Build and Run

### Backend (Signaling Server)

1. Navigate to backend: `cd backend`
2. Build: `cargo build`
3. Run: `cargo run`
   - Server starts on `http://127.0.0.1:3000`
   - WebSocket on `ws://127.0.0.1:3000/ws?token=<JWT>`
   - For WSS (production): Configure TLS with rustls or similar; update ws_url in frontend to `wss://`.

### Frontend (Leptos App)

1. Navigate to frontend: `cd frontend`
2. Build: `trunk build` (generates WASM bundle in dist/)
3. Serve: `trunk serve --open`
   - App starts on `http://127.0.0.1:3001`
   - Update signaling URL in code if backend port changes.

### Full Setup

1. Run backend: `cd backend && cargo run`
2. Run frontend: `cd frontend && trunk serve`
3. Open two browser tabs/windows to `http://127.0.0.1:3001`

## Testing

### Local Testing

1. **Auth**: Register/login with username/password (test: test/test). Note: LocalStorage stores JWT.
2. **Chat**: Navigate to /chat/testroom in both tabs. 
   - One tab acts as initiator (creates offer), the other answers.
   - Check console for ICE candidates, SDP exchange, connection state.
3. **P2P Verification**: Send messages; they should appear in the other tab via data channel (no server relay). Verify "Connected" status.
4. **Reconnection**: Disconnect network (dev tools), reconnect; app should rejoin and renegotiate P2P.
5. **Queuing**: Send message while disconnected; it queues and sends on reconnect.

### Cross-Network P2P

1. Deploy backend to public server (e.g., Render, Fly.io) with TLS for WSS.
2. Update frontend ws_url to wss://your-domain.com/ws.
3. Test from two different networks; STUN handles NAT traversal.

### Cross-Browser

- **Supported**: Chrome, Firefox, Safari (WebRTC standard).
- **Test**: Run in each browser; fallback if needed (e.g., check RTCPeerConnection availability).
- **Notes**: Safari may require HTTPS for WebRTC; use ngrok for local HTTPS testing.

## Security Notes

- **Development**: Uses ws:// and plain passwords (for demo). Production: WSS, hash passwords (bcrypt), secure JWT secret, rate limit auth.
- **Encryption**: WebRTC data channels use DTLS for E2E encryption.
- **Validation**: Server validates inputs; frontend sanitizes.

## Troubleshooting

- **WebRTC fails**: Check console for ICE errors; ensure STUN reachable, firewall allows UDP.
- **WS connection**: Verify backend running, JWT valid.
- **Build errors**: Run `cargo check` in each dir; ensure WASM target installed.

## Deployment

- **Backend**: Deploy to VPS/cloud with TLS cert (Let's Encrypt).
- **Frontend**: Host static files (dist/) on CDN/Netlify; update signaling URL.
- **Full Stack**: Use Docker for backend, CI/CD for frontend.

The app is now ready for use. For extensions, add file sharing, voice, or group chats.
