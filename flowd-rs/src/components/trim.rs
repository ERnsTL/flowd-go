use std::sync::{Condvar, Arc, Mutex};
use crate::{condvar_block, condvar_notify, ProcessEdgeSource, ProcessEdgeSink, Component, ProcessSignalSink, ProcessSignalSource, GraphInportOutportHolder, ProcessInports, ProcessOutports, ComponentComponentPayload, ComponentPort};

pub struct TrimComponent {
    inn: ProcessEdgeSource,
    out: ProcessEdgeSink,
    signals_in: ProcessSignalSource,
    signals_out: ProcessSignalSink,
    graph_inout: Arc<Mutex<GraphInportOutportHolder>>,
    wakeup_notify: Arc<(Mutex<bool>, Condvar)>,
}

impl Component for TrimComponent {
    fn new(mut inports: ProcessInports, mut outports: ProcessOutports, signals_in: ProcessSignalSource, signals_out: ProcessSignalSink, graph_inout: Arc<Mutex<GraphInportOutportHolder>>, wakeup_notify: Arc<(Mutex<bool>, Condvar)>) -> Self where Self: Sized {
        TrimComponent {
            inn: inports.remove("IN").expect("found no IN inport"),
            out: outports.remove("OUT").expect("found no OUT outport"),
            signals_in: signals_in,
            signals_out: signals_out,
            graph_inout: graph_inout,
            wakeup_notify: wakeup_notify,
        }
    }

    fn run(mut self) {
        debug!("Trim is now run()ning!");
        let inn = &mut self.inn;    //TODO optimize
        let out = &mut self.out.sink;
        let out_wakeup = self.out.wake_notify;
        loop {
            trace!("begin of iteration");
            // check signals
            if let Ok(ip) = self.signals_in.try_recv() {
                trace!("received signal ip: {}", std::str::from_utf8(&ip).expect("invalid utf-8"));
                // stop signal
                if ip == b"stop" {   //TODO optimize comparison
                    info!("got stop signal, exiting");
                    break;
                } else if ip == b"ping" {
                    trace!("got ping signal, responding");
                    self.signals_out.send(b"pong".to_vec()).expect("cloud not send pong");
                } else {
                    warn!("received unknown signal ip: {}", std::str::from_utf8(&ip).expect("invalid utf-8"))
                }
            }
            // check in port
            loop {
                if let Ok(ip) = inn.pop() {
                    // read packet - expecting UTF-8 string
                    let mut text = std::str::from_utf8(&ip).expect("non utf-8 data");
                    debug!("got a text to trim: {}", &text);

                    // trim string
                    debug!("len before trim: {}", text.len());
                    text = text.trim();
                    debug!("len after trim: {}", text.len());

                    // send it
                    debug!("forwarding trimmed string...");
                    //TODO optimize .to_vec() copies the contents - is Vec::from faster?
                    out.push(Vec::from(text)).expect("could not push into OUT");
                    condvar_notify!(&*out_wakeup);
                    debug!("done");
                } else {
                    break;
                }
            }

            // are we done?
            if inn.is_abandoned() {
                info!("EOF on inport, shutting down");
                condvar_notify!(&*out_wakeup);
                break;
            }

            trace!("-- end of iteration");
            //###thread::park();
            condvar_block!(&*self.wakeup_notify);
        }
        info!("exiting");
    }

    fn get_metadata() -> ComponentComponentPayload where Self: Sized {
        ComponentComponentPayload {
            name: String::from("Trim"),
            description: String::from("Reads IPs as UTF-8 strings and trims whitespace at beginning and end, forwarding the trimmed string."),
            icon: String::from("cut"),
            subgraph: false,
            in_ports: vec![
                ComponentPort {
                    name: String::from("IN"),
                    allowed_type: String::from("any"),
                    schema: None,
                    required: true,
                    is_arrayport: false,
                    description: String::from("IPs with strings to trim, one string per IP"),
                    values_allowed: vec![],
                    value_default: String::from("")
                }
            ],
            out_ports: vec![
                ComponentPort {
                    name: String::from("OUT"),
                    allowed_type: String::from("any"),
                    schema: None,
                    required: true,
                    is_arrayport: false,
                    description: String::from("trimmed strings"),
                    values_allowed: vec![],
                    value_default: String::from("")
                }
            ],
            ..Default::default()
        }
    }
}