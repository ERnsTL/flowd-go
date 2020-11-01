#![feature(duration_constants)]

use std::net::{TcpListener, TcpStream};
use std::thread::spawn;
use std::time::Duration;

use tungstenite::handshake::server::{Request, Response};
use tungstenite::handshake::HandshakeRole;
use tungstenite::{accept_hdr, Error, HandshakeError, Message, Result};

extern crate pretty_env_logger;
#[macro_use]
extern crate log;

use serde::{Deserialize, Serialize};

fn must_not_block<Role: HandshakeRole>(err: HandshakeError<Role>) -> Error {
    match err {
        HandshakeError::Interrupted(_) => panic!("Bug: blocking socket would block"),
        HandshakeError::Failure(f) => f,
    }
}

fn handle_client(stream: TcpStream) -> Result<()> {
    stream
        .set_write_timeout(Some(Duration::SECOND))
        .expect("set_write_timeout call failed");
    //stream.set_nodelay(true).expect("set_nodelay call failed");

    let callback = |req: &Request, mut response: Response| {
        debug!("Received a new ws handshake");
        debug!("The request's path is: {}", req.uri().path());
        debug!("The request's headers are:");
        for (ref key, value) in req.headers() {
            debug!("  {}: {:?}", key, value);
        }

        // Let's add an additional header to our response to the client.
        let headers = response.headers_mut();
        //TODO check for noflo on Request -- yes, noflo-ui sends websocket sub-protocol request "noflo"
        //TODO it seems that the sec-websocket-protocol does not get sent when setting it this way
        //TODO "sent non-empty 'Sec-WebSocket-Protocol' header but no response was received" -> server should choose if non-empty
        headers.insert("sec-websocket-protocol", "noflo".parse().unwrap()); // not required by noflo-ui
        headers.append("MyCustomHeader", ":)".parse().unwrap());
        headers.append("SOME_TUNGSTENITE_HEADER", "header_value".parse().unwrap()); //TODO remove these

        Ok(response)
    };
    //let mut socket = accept(stream).map_err(must_not_block)?;
    let mut websocket = accept_hdr(stream, callback).map_err(must_not_block)?;

    //TODO wss
    //TODO check secret

    info!("entering receive loop");
    loop {
        info!("waiting for next message");
        match websocket.read_message()? {
            msg @ Message::Text(_) | msg @ Message::Binary(_) => {
                info!("got a text|binary message");
                //debug!("message data: {}", msg.clone().into_text().unwrap());

                let fbpmsg: FBPMessage = serde_json::from_slice(msg.into_data().as_slice())
                    .expect("failed to decode JSON message"); //TODO data handover optimizable?

                match fbpmsg {
                    FBPMessage::RuntimeGetruntimeMessage(payload) => {
                        info!(
                            "got runtime/getruntime message with secret {}",
                            payload.secret
                        );
                        // send response = runtime/runtime message
                        info!("response: sending runtime/runtime message");
                        websocket
                            .write_message(Message::text(
                                serde_json::to_string(&RuntimeRuntimeMessage::default())
                                    .expect("failed to serialize runtime/runtime message"),
                            ))
                            .expect("failed to write message into websocket");
                        // (specification) "If the runtime is currently running a graph and it is able to speak the full Runtime protocol, it should follow up with a ports message."
                        info!("response: sending runtime/ports message");
                        websocket
                            .write_message(Message::text(
                                serde_json::to_string(&RuntimePortsMessage::default())
                                    .expect("failed to serialize runtime/ports message"),
                            ))
                            .expect("failed to write message into websocket");
                    }

                    FBPMessage::ComponentListMessage(payload) => {
                        info!("got component/list message");
                        info!("response: sending component/component message");
                        websocket
                            .write_message(Message::text(
                                serde_json::to_string(&ComponentComponentMessage::default())
                                    .expect("failed to serialize component/component message"),
                            ))
                            .expect("failed to write message into websocket");
                        info!("response: sending component/componentsready message");
                        websocket
                            .write_message(Message::text(
                                serde_json::to_string(&ComponentComponentsreadyMessage::default())
                                    .expect(
                                        "failed to serialize component/componentsready message",
                                    ),
                            ))
                            .expect("failed to write message into websocket");
                    }

                    _ => {
                        info!("unknown message type received: {:?}", fbpmsg); //TODO wanted Display trait here
                        websocket.close(None).expect("could not close websocket");
                    }
                }

                //websocket.write_message(msg)?;
            }
            Message::Ping(_) | Message::Pong(_) => {
                info!("got a ping|pong");
            }
            Message::Close(_) => {
                info!("got a close, breaking");
                break;
            }
        }
    }
    //websocket.close().expect("could not close websocket");
    info!("---");
    Ok(())
}

fn main() {
    pretty_env_logger::init();

    let server = TcpListener::bind("localhost:3569").unwrap();

    info!("listening");
    for stream in server.incoming() {
        spawn(move || match stream {
            Ok(stream) => {
                info!("got a client");
                if let Err(err) = handle_client(stream) {
                    match err {
                        Error::ConnectionClosed | Error::Protocol(_) | Error::Utf8 => (),
                        e => error!("test: {}", e),
                    }
                }
            }
            Err(e) => error!("Error accepting stream: {}", e),
        });
    }
}

//TODO currently panicks if unknown variant
#[derive(Deserialize, Debug)]
#[serde(tag = "command", content = "payload")] //TODO multiple tags: protocol and command
enum FBPMessage {
    #[serde(rename = "getruntime")]
    RuntimeGetruntimeMessage(RuntimeGetruntimePayload),
    //#[serde(rename = "runtime")]
    RuntimeRuntimeMessage,
    //#[serde(rename = "ports")]
    RuntimePortsMessage,
    #[serde(rename = "list")]
    ComponentListMessage(ComponentListPayload),
    //#[serde(rename = "component")]
    ComponentComponentMessage,
    //#[serde(rename = "componentsready")]
    ComponentComponentsreadyMessage,
}

#[derive(Deserialize, Debug)]
struct RuntimeGetruntimePayload {
    secret: String,
}

#[derive(Serialize, Debug)]
struct RuntimeRuntimeMessage {
    protocol: String, // group of messages (and capabities)
    command: String,  // name of message within group
    payload: RuntimeRuntimePayload,
}

impl Default for RuntimeRuntimeMessage {
    fn default() -> Self {
        RuntimeRuntimeMessage {
            protocol: String::from("runtime"),
            command: String::from("runtime"),
            payload: RuntimeRuntimePayload::default(),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RuntimeRuntimePayload {
    id: String,                        // UUID of this runtime instance
    label: String,                     // human-readable description of the runtime
    version: String,                   // supported protocol version
    all_capabilities: Vec<Capability>, // capabilities supported by runtime
    capabilities: Vec<Capability>, // capabities for you //TODO implement privilege level restrictions
    graph: String,                 // currently active graph
    #[serde(rename = "type")]
    runtime: String, // name of the runtime software, "flowd"
    namespace: String,             // namespace of components for this project of top-level graph
    repository: String,            // source code repository of this runtime software
    repository_version: String,    // repository version of this software build
}

impl Default for RuntimeRuntimePayload {
    fn default() -> Self {
        RuntimeRuntimePayload {
            id: String::from("f18a4924-9d4f-414d-a37c-deadbeef0000"),
            label: String::from("human-readable description of the runtime"),
            version: String::from("0.7"),
            all_capabilities: vec![
                Capability::ProtocolRuntime,
                Capability::ProtocolNetwork,
                Capability::GraphReadonly,
                Capability::ProtocolComponent,
                Capability::NetworkStatus,
                Capability::NetworkPersist,
            ],
            capabilities: vec![
                Capability::ProtocolRuntime,
                Capability::GraphReadonly,
                Capability::ProtocolComponent,
                Capability::NetworkStatus,
            ],
            graph: String::from("default_graph"),
            runtime: String::from("flowd"),
            namespace: String::from("main"),
            repository: String::from("https://github.com/ERnsTL/flowd.git"),
            repository_version: String::from("0.0.1-ffffffff"), //TODO use actual git commit and acutal version
        }
    }
}

#[derive(Serialize, Debug)]
enum Capability {
    #[serde(rename = "protocol:runtime")]
    ProtocolRuntime,
    #[serde(rename = "protocol:network")]
    ProtocolNetwork,
    #[serde(rename = "graph:readonly")]
    GraphReadonly,
    #[serde(rename = "protocol:component")]
    ProtocolComponent,
    #[serde(rename = "network:status")]
    NetworkStatus,
    #[serde(rename = "network:persist")]
    NetworkPersist,
}

#[derive(Serialize, Debug)]
struct RuntimePortsMessage {
    protocol: String,
    command: String,
    payload: RuntimePortsPayload,
}

impl Default for RuntimePortsMessage {
    fn default() -> Self {
        RuntimePortsMessage {
            protocol: String::from("runtime"),
            command: String::from("ports"),
            payload: RuntimePortsPayload::default(),
        }
    }
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
struct RuntimePortsPayload {
    graph: String,
    in_ports: Vec<String>,
    out_ports: Vec<String>,
}

impl Default for RuntimePortsPayload {
    fn default() -> Self {
        RuntimePortsPayload {
            graph: String::from("default_graph"),
            in_ports: vec![],
            out_ports: vec![],
        }
    }
}

#[derive(Serialize, Debug)]
struct ComponentComponentMessage {
    protocol: String,
    command: String,
    payload: ComponentComponentPayload,
}

impl Default for ComponentComponentMessage {
    fn default() -> Self {
        ComponentComponentMessage {
            protocol: String::from("component"),
            command: String::from("component"),
            payload: ComponentComponentPayload::default(),
        }
    }
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct ComponentListPayload {
    secret: String,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
struct ComponentComponentPayload {
    name: String, // spec: component name in format that can be used in graphs. Should contain the component library prefix.
    description: String,
    icon: String, // spec: visual icon for the component, matching icon names in Font Awesome
    subgraph: bool, // spec: is the component a subgraph?
    in_ports: Vec<String>, //TODO create classes
    out_ports: Vec<String>, //TODO create classes
}

impl Default for ComponentComponentPayload {
    fn default() -> Self {
        ComponentComponentPayload {
            name: String::from("main/Repeat"), //TODO Repeat, Drop, Output
            description: String::from("description of the Repeat component"),
            icon: String::from("usd"), //TODO with fa- prefix?
            subgraph: false,
            in_ports: vec![],
            out_ports: vec![],
        }
    }
}

#[derive(Serialize, Debug)]
struct ComponentComponentsreadyMessage {
    protocol: String,
    command: String,
    payload: ComponentComponentsreadyPayload,
}

impl Default for ComponentComponentsreadyMessage {
    fn default() -> Self {
        ComponentComponentsreadyMessage {
            protocol: String::from("component"),
            command: String::from("componentsready"),
            payload: ComponentComponentsreadyPayload::default(),
        }
    }
}

#[derive(Serialize, Debug)]
struct ComponentComponentsreadyPayload {
    // no playload fields; spec: indication that a component listing has finished
}

impl Default for ComponentComponentsreadyPayload {
    fn default() -> Self {
        ComponentComponentsreadyPayload {}
    }
}
