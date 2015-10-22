extern crate rustbox;

use std::io::{BufReader, BufRead, Write};
use std::net::TcpStream;

use rustbox::{Color, RustBox, Event, Key};

#[derive(Debug)]
struct Constraint {
    search_type: String,
    search_term: String,
}

impl Constraint {
    pub fn new(srch_type: String ) -> Constraint {
        Constraint {search_type: srch_type, search_term: String::new() }
    }

    fn to_mpd_string(&self) -> String {
        format!("{} \"{}\" ", self.search_type, self.search_term)
    }

    fn to_display_string(&self) -> String {
        format!("{}: {}", self.search_type, self.search_term)
    }
}

struct MPC {
    connection: TcpStream,
    reader: BufReader<TcpStream>,
}

impl MPC {
    pub fn new() -> MPC {
        let mut buf = String::new();
        let write_conn = match TcpStream::connect("localhost:6600") {
            Ok(v) => v,
            Err(e) => panic!("Could not connect to MPD.\n{}", e)
        };
        let read_conn = match write_conn.try_clone() {
            Ok(v) => v,
            Err(e) => panic!("Could not clone MPD connection for reading\n{}", e)
        };
        //intentionally transferring ownership
        let mut reader = BufReader::new(read_conn);
        reader.read_line(&mut buf).unwrap();
        assert!(buf.starts_with("OK"), "MPD connected, but did not return OK upon connection");
        MPC {
            connection: write_conn,
            reader: reader
        }
    }

    fn send_command(&mut self, command: &str) -> Vec<String> {
        self.connection.write(command.as_bytes()).ok().expect("Error writing to MPD");
        self.connection.write("\n".as_bytes()).ok().expect("Error writing to MPD");
        let mut results: Vec<String> = Vec::new();
        let mut buf = String::new();
        loop {
            buf.clear();
            self.reader.read_line(&mut buf).ok().expect("Error reading from MPD");
            if buf == "OK\n" {
                return results;
            } else if buf.starts_with("ACK") {
                panic!("MPD returned an ACK for command {}", command);
            } else {
                results.push(String::from(buf.trim()));
            }
        }
    }
}

fn char_to_mpd_type(c: char) -> Option<String> {
    match c {
        ' ' => Some("any"),
        't' => Some("title"),
        'T' => Some("track"),
        'd' => Some("disc"),
        'b' => Some("album"),
        'a' => Some("artist"),
        'A' => Some("albumartist"),
        '\n' => Some(""), //end of input
        _ => None
    }.map(|x| x.to_string())
}

fn print_screen(constraints: &Vec<Constraint>, files: &Vec<String>, rustbox: &RustBox) {
    let mut y = 1;
    let height = rustbox.height();
    rustbox.clear();
    rustbox.print(0, 0, rustbox::RB_BOLD, Color::Default, Color::Default, "Filters:");
    
    for constraint in constraints {
        rustbox.print(0, y, rustbox::RB_BOLD, Color::Default, Color::Default, &constraint.to_display_string());
        y += 1;
        if y == (height - 1) {
            rustbox.present();
            return;
        }
    }
    
    for file in files {
        rustbox.print(0, y, rustbox::RB_BOLD, Color::Default, Color::Default, &file);
        y += 1;
        if y == (height - 1) {
            rustbox.present();
            return;
        }
    }
    rustbox.present();
}

enum State {
    NeedType,
    NeedString,
    ShouldExit,
    ShouldCommit
}

impl State {
    fn is_exit_state(&self) -> bool {
        match *self {
            State::ShouldExit => true,
            State::ShouldCommit => true,
            _ => false
        }
    }
}
fn main() {
    let mut mpc = MPC::new();

    let mut constraints: Vec<Constraint> = Vec::new();
    let mut state = State::NeedType;

    {
        //using rustbox now
        let rustbox = RustBox::init(Default::default()).ok().expect("Error initializing rustbox");
        while !state.is_exit_state() {
            //get update from mpd
            let matched_files = match constraints.is_empty() {
                true => Vec::new(),
                false => {
                    let mut query = String::from("search ");
                    for constraint in &constraints {
                        query.push_str(&constraint.to_mpd_string());
                    };
                    mpc.send_command(&query).into_iter().filter(|x| x.starts_with("file")).collect()
                }
            };
            
            //update display
            print_screen(&constraints, &matched_files, &rustbox);

            //get input
            let key = match rustbox.poll_event(false) {
                Ok(Event::KeyEvent(key)) => key,
                Ok(_) => None,
                Err(e) => panic!("Error with rustbox {}",e)
            };

            //ungodly state transition matrix
            state = match (state, key) {
                //handle all cases we should never reach
                (State::ShouldExit,   _) => panic!("unreachable code reached!"),
                (State::ShouldCommit, _) => panic!("unreachable code reached!"),
                //handle all cases where we got no input
                (State::NeedType,     None) => State::NeedType,
                (State::NeedString,   None) => State::NeedString,
                (_, Some(Key::Esc)) => State::ShouldExit,
                //handle all cases for NeedType
                (State::NeedType, Some(Key::Enter)) => State::ShouldCommit,
                (State::NeedType, Some(Key::Backspace)) => match constraints.is_empty() {
                        true  => State::NeedType, //lready at the end
                        false => {
                                constraints.pop();
                                State::NeedString
                            }
                    },
                (State::NeedType, Some(Key::Char(x))) => match char_to_mpd_type(x) {
                        Some(str) => {
                                constraints.push(Constraint::new(str));
                                State::NeedString
                            },
                        None => State::NeedType
                    },
                (State::NeedType, _) => State::NeedType,
                //
                (State::NeedString, Some(Key::Enter)) => State::NeedType,
                (State::NeedString, Some(Key::Backspace)) => match constraints.is_empty() {
                        false => {
                                constraints.last_mut().unwrap().search_term.pop();
                                State::NeedString
                            },
                        true => State::NeedType
                    },
                (State::NeedString, Some(Key::Char(x))) if x.is_alphanumeric() || x.is_whitespace() => {
                        constraints.last_mut().unwrap().search_term.push(x);
                        State::NeedString
                    },
                (State::NeedString, _) => State::NeedString
            };
        }
    }//end using rustbox

    //get the current playlist length
    let last_pos: String = mpc.send_command("status")
        .into_iter()
        .find(|x| x.starts_with("playlistlength: "))
        .unwrap()
        .split(' ')
        .last()
        .unwrap()
        .into();
    
    if constraints.is_empty() { //no songs, nothing to do
        return ;
    }

    let mut query = String::from("searchadd ");
    for constraint in &constraints {
        query.push_str(&constraint.to_mpd_string());
    };
    mpc.send_command(&query);

    query = format!("play {}", last_pos);
    mpc.send_command(&query);

}
