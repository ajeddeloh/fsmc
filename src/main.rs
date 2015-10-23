extern crate rustbox;

use std::io;
use std::io::{BufReader, BufRead, Write, Error, ErrorKind};
use std::net::TcpStream;

use rustbox::{Color, RustBox, Event, Key};

struct Constraint {
    search_type: String,
    search_term: String,
}

impl Constraint {
    pub fn new(metadata_type: char ) -> Constraint {
        let srch_type = String::from(match metadata_type {
            ' ' => "any",
            't' => "title",
            'T' => "track",
            'd' => "disc",
            'b' => "album",
            'a' => "artist",
            'A' => "albumartist",
            _ => panic!("Tried to create constraint from invalid metadata type")
        });
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
    pub fn new() -> io::Result<MPC> {
        let mut buf = String::new();
        let write_conn = try!(TcpStream::connect("localhost:6600"));
        let read_conn = try!(write_conn.try_clone());
        
        //intentionally transferring ownership
        let mut reader = BufReader::new(read_conn);
        try!(reader.read_line(&mut buf));
        if !buf.starts_with("OK") {
            return Err(Error::new(ErrorKind::Other, "Mpd did not return OK"));
        }
        Ok(MPC {
            connection: write_conn,
            reader: reader
        })
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

fn char_is_mpd_type(c: char) -> bool {
    match c {
        ' ' => true,
        't' => true,
        'T' => true,
        'd' => true,
        'b' => true,
        'a' => true,
        'A' => true,
        _ => false
    }
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
    let mut mpc = match MPC::new() { 
        Ok(m) => m,
        Err(_) => panic!("Panicing here isn't too bad is it?")
    };

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

            //get input, discard everything but key events
            let key = match rustbox.poll_event(false) {
                Ok(Event::KeyEvent(key)) => key,
                Ok(_) => None,
                Err(e) => panic!("Error with rustbox {}",e)
            };

            //invariant: You can only be in State::NeedString if constraints is non-empty
            state = match state {
                State::ShouldExit => panic!("Unreachable code reached!"),
                State::ShouldCommit => panic!("Unreachable code reached!"),
                State::NeedType => match key {
                    None => State::NeedType,
                    Some(k) => match k {
                        Key::Esc => State::ShouldExit,
                        Key::Enter => State::ShouldCommit,
                        Key::Backspace => match constraints.pop() {
                            None => State::NeedType,
                            Some(_) => State::NeedString,
                        },
                        Key::Char(x) if char_is_mpd_type(x) => {
                            constraints.push(Constraint::new(x));
                            State::NeedString
                        },
                        _ => State::NeedType,
                    }
                },
                State::NeedString => match key {
                    None => State::NeedString,
                    Some(k) => match k {
                        Key::Esc => State::ShouldExit,
                        Key::Enter => State::NeedType,
                        Key::Backspace => match constraints.last_mut().unwrap().search_term.pop() {
                            None => {
                                constraints.pop();
                                State::NeedType
                            },
                            Some(_) => State::NeedString
                        },
                        Key::Char(x) if x.is_alphanumeric() || x.is_whitespace() => {
                            constraints.last_mut().unwrap().search_term.push(x);
                            State::NeedString
                        },
                        _ => State::NeedString
                    }
                }
            }
        }
    }//end using rustbox, needed if we want to print errors since rustbox hijacks the term

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
