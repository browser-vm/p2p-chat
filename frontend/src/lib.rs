use leptos::*;
use leptos_meta::*;
use leptos_router::*;
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Message {
    pub content: String,
    pub sender: String,
    pub timestamp: String,
}

#[component]
fn App() -> impl IntoView {
    view! {
        <Stylesheet id="leptos" href="/pkg/p2p_chat_frontend.css"/>
        <Title text="P2P Chat"/>
        <Link rel="shortcut icon" type_="image/ico" href="/favicon.ico"/>
        <Router fallback=|| view! { <div>"Not Found"</div> }>
            <header>
                <h1>"P2P Chat App"</h1>
            </header>
            <main>
                <Routes>
                    <Route path="/" view=HomePage/>
                    <Route path="/login" view=LoginPage/>
                    <Route path="/register" view=RegisterPage/>
                    <Route path="/chat/:room" view=ChatPage/>
                </Routes>
            </main>
        </Router>
    }
}

#[component]
fn HomePage() -> impl IntoView {
    let navigate = use_navigate();

    view! {
        <div class="home">
            <h2>"Welcome to P2P Chat"</h2>
            <p>"Secure peer-to-peer messaging with end-to-end encryption."</p>
            <div class="buttons">
                <button on:click=move |_| navigate("/login", false).unwrap()>"Login"</button>
                <button on:click=move |_| navigate("/register", false).unwrap()>"Register"</button>
            </div>
        </div>
    }
}

#[component]
fn LoginPage() -> impl IntoView {
    let navigate = use_navigate();
    let (username, set_username) = create_signal("".to_string());
    let (password, set_password) = create_signal("".to_string());

    let on_submit = create_action(move |()| {
        let username = username.get();
        let password = password.get();
        async move {
            // In real impl, call backend /login
            if username == "test" && password == "test" {
                // Store JWT in localStorage
                set_username.set("".to_string());
                set_password.set("".to_string());
                navigate("/chat/testroom", false).unwrap();
            }
        }
    });

    view! {
        <div class="auth-form">
            <h2>"Login"</h2>
            <form on:submit=|ev| on_submit.dispatch(ev)>
                <input
                    type="text"
                    placeholder="Username"
                    prop:value=username
                    on:input=move |ev| set_username.set(event_target_value(&ev))
                />
                <input
                    type="password"
                    placeholder="Password"
                    prop:value=password
                    on:input=move |ev| set_password.set(event_target_value(&ev))
                />
                <button type="submit">"Login"</button>
            </form>
            <p>
                <a href="/register">"Don't have an account? Register"</a>
            </p>
        </div>
    }
}

#[component]
fn RegisterPage() -> impl IntoView {
    let navigate = use_navigate();
    let (username, set_username) = create_signal("".to_string());
    let (password, set_password) = create_signal("".to_string());

    let on_submit = create_action(move |()| {
        let username = username.get();
        let password = password.get();
        async move {
            // In real impl, call backend /register
            if !username.is_empty() && !password.is_empty() {
                set_username.set("".to_string());
                set_password.set("".to_string());
                navigate("/login", false).unwrap();
            }
        }
    });

    view! {
        <div class="auth-form">
            <h2>"Register"</h2>
            <form on:submit=|ev| on_submit.dispatch(ev)>
                <input
                    type="text"
                    placeholder="Username"
                    prop:value=username
                    on:input=move |ev| set_username.set(event_target_value(&ev))
                />
                <input
                    type="password"
                    placeholder="Password"
                    prop:value=password
                    on:input=move |ev| set_password.set(event_target_value(&ev))
                />
                <button type="submit">"Register"</button>
            </form>
            <p>
                <a href="/login">"Already have an account? Login"</a>
            </p>
        </div>
    }
}

#[component]
fn ChatPage() -> impl IntoView {
    let params = use_params_map();
    let room = move || params.with(|p| p.get("room").cloned().unwrap_or_default());

    let (messages, set_messages) = create_signal::<Vec<Message>, _>(vec![]);
    let (input, set_input) = create_signal("".to_string());
    let (connection_status, set_connection_status) = create_signal("Disconnected".to_string());
    let (data_channel, set_data_channel) = create_signal<Option<web_sys::RtcDataChannel>>(None);
    let (peer_connection, set_peer_connection) = create_signal<Option<web_sys::RtcPeerConnection>>(None);
    let (ws, set_ws) = create_signal<Option<web_sys::WebSocket>>(None);
    let (is_initiator, set_is_initiator) = create_signal(false);
    let (queued_messages, set_queued_messages) = create_signal::<Vec<String>, _>(vec![]);

    // Get JWT from localStorage
    let jwt = use_memo(move || {
        let window = web_sys::window().unwrap();
        let storage = window.local_storage().unwrap().unwrap();
        storage.get_item("jwt").unwrap().unwrap_or_default()
    });

    // Initialize peer connection
    create_effect(move |_| {
        let config = web_sys::RtcConfiguration::new(&js_sys::Array::new());
        let ice_server = web_sys::RtcIceServer::new("stun:stun.l.google.com:19302");
        config.ice_servers(&js_sys::Array::of1(&ice_server.into()));
        let pc = web_sys::RtcPeerConnection::new_with_configuration(&config).unwrap();
        set_peer_connection.set(Some(pc));
    });

    let pc = peer_connection;

    // Create data channel
    let create_data_channel = move || {
        if let Some(pc) = pc() {
            let label = "chat".to_string();
            let dc_init = web_sys::RtcDataChannelInit::new();
            dc_init.set_ordered(true);
            dc_init.set_max_retransmits(0);
            let dc = pc.create_data_channel_with_data_channel_init(&label, &dc_init).unwrap();
            dc.set_binary_type(web_sys::RtcDataChannelBinaryType::Arraybuffer);
            dc.set_onopen(Some(wasm_bindgen::closure::Closure::wrap(Box::new(move |_ev| {
                set_connection_status.set("Connected".to_string());
                console::log_1(&"Data channel open".into());
                // Send queued messages
                set_queued_messages.update(|q| {
                    for msg in q.drain(..) {
                        dc.send_with_str(&msg);
                    }
                });
            }) as Box<dyn FnMut(web_sys::RtcDataChannelEvent)>).forget()));
            dc.set_onclose(Some(wasm_bindgen::closure::Closure::wrap(Box::new(move |_ev| {
                set_connection_status.set("Disconnected".to_string());
                console::log_1(&"Data channel closed".into());
            }) as Box<dyn FnMut(web_sys::RtcDataChannelEvent)>).forget()));
            dc.set_onmessage(Some(wasm_bindgen::closure::Closure::wrap(Box::new(move |ev| {
                if let Ok(data) = ev.data().as_string() {
                    set_messages.update(|msgs| msgs.push(Message {
                        content: data.clone(),
                        sender: "peer".to_string(),
                        timestamp: js_sys::Date::new_0().to_string(),
                    }));
                }
            }) as Box<dyn FnMut(web_sys::MessageEvent)>).forget()));
            dc.set_onerror(Some(wasm_bindgen::closure::Closure::wrap(Box::new(move |_ev| {
                console::error_1(&"Data channel error".into());
            }) as Box<dyn FnMut(web_sys::RtcDataChannelEvent)>).forget()));
            set_data_channel.set(Some(dc));
        }
    };

    // ICE candidate handler
    create_effect(move |_| {
        if let Some(pc) = pc() {
            let room = room();
            let ws = ws();
            let closure = Closure::wrap(Box::new(move |ev: web_sys::RtcPeerConnectionIceEvent| {
                if let Some(candidate) = ev.candidate() {
                    let candidate_init = RtcIceCandidateInit::new(&candidate.to_json().unwrap());
                    let candidate_str = JSON::stringify(&candidate_init).unwrap().as_string().unwrap();
                    let ice_msg = serde_wasm_bindgen::to_value(&SignalingMessage::IceCandidate { room: room.clone(), candidate: candidate_str }).unwrap();
                    if let Some(ws) = ws.as_ref() {
                        let _ = ws.send_with_json(&ice_msg);
                    }
                }
            }) as Box<dyn FnMut(web_sys::RtcPeerConnectionIceEvent)>);
            pc.set_onicecandidate(Some(closure.as_ref().unchecked_ref()));
            closure.forget();
        }
    });

    // Connection state handler
    create_effect(move |_| {
        if let Some(pc) = pc() {
            let set_status = set_connection_status;
            let closure = Closure::wrap(Box::new(move |ev: web_sys::Event| {
                let state = pc.connection_state();
                set_status.set(state.as_string().unwrap_or("Unknown".to_string()));
            }) as Box<dyn FnMut(web_sys::Event)>);
            pc.set_onconnectionstatechange(Some(closure.as_ref().unchecked_ref()));
            closure.forget();
        }
    });

    // Connect to signaling server
    let connect_signaling = move |jwt: String, room_name: String| {
        let ws_url = format!("ws://localhost:3000/ws?token={}", jwt);
        let ws = web_sys::WebSocket::new(&ws_url).unwrap();
        ws.set_onopen(Some(wasm_bindgen::closure::Closure::wrap(Box::new(move |_ev| {
            let join_msg = serde_wasm_bindgen::to_value(&SignalingMessage::JoinRoom { room: room_name.clone() }).unwrap();
            ws.send_with_json(&join_msg).unwrap();
            console::log_1(&"Joined room".into());
        }) as Box<dyn FnMut(web_sys::Event)>).forget()));
        ws.set_onmessage(Some(wasm_bindgen::closure::Closure::wrap(Box::new(move |ev: web_sys::MessageEvent| {
            if let Ok(json_str) = ev.data().as_string() {
                if let Ok(msg) = serde_json::from_str::<SignalingMessage>(&json_str) {
                    match msg {
                        SignalingMessage::Peers { peers } => {
                            if peers.len() == 2 {
                                create_data_channel();
                                if is_initiator() {
                                    create_offer(room_name.clone());
                                }
                            }
                        }
                        SignalingMessage::Offer { room: _, sdp } => {
                            handle_offer(sdp, room_name.clone());
                        }
                        SignalingMessage::Answer { room: _, sdp } => {
                            handle_answer(sdp);
                        }
                        SignalingMessage::IceCandidate { room: _, candidate } => {
                            handle_ice_candidate(&candidate);
                        }
                        SignalingMessage::Error { message } => {
                            console::error_1(&message.into());
                        }
                        _ => {}
                    }
                }
            }
        }) as Box<dyn FnMut(web_sys::MessageEvent)>).forget()));
        ws.set_onclose(Some(wasm_bindgen::closure::Closure::wrap(Box::new(move |_ev| {
            set_connection_status.set("Disconnected".to_string());
            console::log_1(&"Signaling disconnected".into());
        }) as Box<dyn FnMut(web_sys::CloseEvent)>).forget()));
        ws.set_onerror(Some(wasm_bindgen::closure::Closure::wrap(Box::new(move |_ev| {
            console::error_1(&"Signaling error".into());
        }) as Box<dyn FnMut(web_sys::Event)>).forget()));
        set_ws.set(Some(ws));
    };

    let create_offer = move |room_name: String| {
        if let Some(pc) = pc() {
            let options = RtcOfferAnswerOptions::new();
            let promise = pc.create_offer_with_rtc_offer_options(&options);
            spawn_local(async move {
                let result = JsFuture::from(promise).await;
                if let Ok(sdp_obj) = result.dyn_into::<js_sys::Object>() {
                    let sdp_str = JSON::stringify(&sdp_obj).unwrap().as_string().unwrap();
                    let _ = pc.set_local_description_with_type(&web_sys::RtcSdpDescription::new(&sdp_str).unwrap(), RtcSdpType::Offer);
                    let offer_msg = serde_wasm_bindgen::to_value(&SignalingMessage::Offer { room: room_name, sdp: sdp_str }).unwrap();
                    if let Some(ws) = ws() {
                        ws.send_with_json(&offer_msg).unwrap();
                    }
                }
            });
        }
    };

    let handle_offer = move |sdp: String, room_name: String| {
        if let Some(pc) = pc() {
            let desc = web_sys::RtcSdpDescription::new(&sdp).unwrap();
            let _ = pc.set_remote_description_with_type(&desc, RtcSdpType::Offer);
            let options = RtcOfferAnswerOptions::new();
            let promise = pc.create_answer_with_rtc_peer_connection_answer_options(&options);
            spawn_local(async move {
                let result = JsFuture::from(promise).await;
                if let Ok(sdp_obj) = result.dyn_into::<js_sys::Object>() {
                    let sdp_str = JSON::stringify(&sdp_obj).unwrap().as_string().unwrap();
                    let _ = pc.set_local_description(&web_sys::RtcSdpDescription::new(&sdp_str).unwrap());
                    let answer_msg = serde_wasm_bindgen::to_value(&SignalingMessage::Answer { room: room_name, sdp: sdp_str }).unwrap();
                    if let Some(ws) = ws() {
                        ws.send_with_json(&answer_msg).unwrap();
                    }
                }
            });
        }
    };

    let handle_answer = move |sdp: String| {
        if let Some(pc) = pc() {
            let desc = web_sys::RtcSdpDescription::new(&sdp).unwrap();
            let _ = pc.set_remote_description(&desc);
        }
    };

    let handle_ice_candidate = move |candidate: &str| {
        if let Some(pc) = pc() {
            let candidate_init = RtcIceCandidateInit::new(candidate);
            let _ = pc.add_ice_candidate_with_rtc_ice_candidate_init(&candidate_init);
        }
    };

    // Connect on mount
    create_effect(move |_| {
        let room_name = room();
        if let Some(jwt_val) = jwt() {
            if !jwt_val.is_empty() {
                connect_signaling(jwt_val, room_name);
            }
        }
    });

    // Reconnection logic
    use_effect(move || {
        let window = web_sys::window().unwrap();
        let closure = Closure::wrap(Box::new(move || {
            console::log_1(&"Network reconnected, attempting to rejoin".into());
            let room_name = room();
            if let Some(jwt_val) = jwt() {
                if !jwt_val.is_empty() {
                    connect_signaling(jwt_val, room_name);
                }
            }
        }) as Box<dyn FnMut()>);
        window.add_event_listener_with_callback("online", closure.as_ref().unchecked_ref()).unwrap();
        move || {
            let _ = window.remove_event_listener_with_callback("online", closure.as_ref().unchecked_ref());
        }
    });

    let on_send = create_action(move |()| {
        let content = input.get();
        async move {
            if !content.is_empty() {
                if let Some(dc) = data_channel() {
                    match dc.ready_state() {
                        RtcDataChannelState::Open => {
                            dc.send_with_str(&content);
                        }
                        _ => {
                            set_queued_messages.update(|q| q.push(content.clone()));
                            console::log_1(&format!("Queued: {}", content).into());
                        }
                    }
                }
                set_messages.update(|msgs| {
                    msgs.push(Message {
                        content: content.clone(),
                        sender: "me".to_string(),
                        timestamp: js_sys::Date::new_0().to_string(),
                    });
                });
                set_input.set("".to_string());
            }
        }
    });

    view! {
        <div class="chat">
            <h2>"Chat Room: " {room}</h2>
            <div class="status">"Connection: " {connection_status}</div>
            <div class="messages">
                <For
                    each=messages
                    key=|msg| msg.timestamp.clone()
                    view=move |msg| view! {
                        <div class=move || if msg.sender == "me" { "message sent" } else { "message received" }>
                            <strong>{msg.sender}:</strong> {msg.content}
                            <small>{msg.timestamp}</small>
                        </div>
                    }
                />
            </div>
            <form on:submit=|ev| on_send.dispatch(ev) prevent_default=true>
                <input
                    type="text"
                    placeholder="Type your message..."
                    prop:value=input
                    on:input=move |ev| set_input.set(event_target_value(&ev))
                />
                <button type="submit">"Send"</button>
            </form>
            <div class="queued">"Queued messages: " {queued_messages.get().len()}</div>
        </div>
    }
}

fn main() {
    console_error_panic_hook::set_once();
    console_log::init_with_level(log::Level::Info).expect("error initializing log");
    mount_to_body(|cx| view! { cx, <App/> })
}