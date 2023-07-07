use std::sync::{Condvar, Arc, Mutex};
use crate::{condvar_block, condvar_notify, ProcessEdgeSource, ProcessEdgeSink, Component, ProcessSignalSink, ProcessSignalSource, GraphInportOutportHolder, ProcessInports, ProcessOutports, ComponentComponentPayload, ComponentPort};

pub struct FileReaderComponent {
    inn: ProcessEdgeSource,
    out: ProcessEdgeSink,
    signals_in: ProcessSignalSource,
    signals_out: ProcessSignalSink,
    graph_inout: Arc<Mutex<GraphInportOutportHolder>>,
    wakeup_notify: Arc<(Mutex<bool>, Condvar)>,
}

impl Component for FileReaderComponent {
    fn new(mut inports: ProcessInports, mut outports: ProcessOutports, signals_in: ProcessSignalSource, signals_out: ProcessSignalSink, graph_inout: Arc<Mutex<GraphInportOutportHolder>>, wakeup_notify: Arc<(Mutex<bool>, Condvar)>) -> Self where Self: Sized {
        FileReaderComponent {
            inn: inports.remove("NAMES").expect("found no NAMES inport"),
            out: outports.remove("OUT").expect("found no OUT outport"),
            signals_in: signals_in,
            signals_out: signals_out,
            graph_inout: graph_inout,
            wakeup_notify: wakeup_notify,
        }
    }

    fn run(mut self) {
        debug!("FileReader is now run()ning!");
        let filenames = &mut self.inn;    //TODO optimize
        let out = &mut self.out.sink;
        let out_wakeup = self.out.wake_notify;
        loop {
            trace!("begin of iteration");
            // check signals
            //TODO optimize, there is also try_recv() and recv_timeout()
            if let Ok(ip) = self.signals_in.try_recv() {
                //TODO optimize string conversions
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
            //TODO while !inn.is_empty() {
            loop {
                if let Ok(ip) = filenames.pop() {
                    // read filename on inport
                    let file_path = std::str::from_utf8(&ip).expect("non utf-8 data");
                    debug!("got a filename: {}", &file_path);

                    // read whole file
                    //TODO may be big file - add chunking
                    //TODO enclose files in brackets to know where its stream of chunks start and end
                    debug!("reading file...");
                    let contents = std::fs::read(file_path).expect("should have been able to read the file");

                    // send it
                    debug!("forwarding file contents...");
                    out.push(contents).expect("could not push into OUT");
                    condvar_notify!(&*out_wakeup);
                    debug!("done");
                } else {
                    break;
                }
            }

            // are we done?
            if filenames.is_abandoned() {
                info!("EOF on inport NAMES, shutting down");
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
            name: String::from("FileReader"),
            description: String::from("Reads the contents of the given files and sends the contents."),
            icon: String::from("file"),
            subgraph: false,
            in_ports: vec![
                ComponentPort {
                    name: String::from("NAMES"),
                    allowed_type: String::from("any"),
                    schema: None,
                    required: true,
                    is_arrayport: false,
                    description: String::from("filenames, one per IP"),
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
                    description: String::from("conents of the given files"),
                    values_allowed: vec![],
                    value_default: String::from("")
                }
            ],
            ..Default::default()
        }
    }
}