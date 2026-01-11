use log::{trace, debug};
use regex::Regex;
use std::error::Error;
pub struct OuputAnalyzer<'a> {
    response: &'a str,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ResponseParts {
   pub pretext: String,
   pub  title: String,
   pub message: String,
   pub things_of_interest: Vec<String>,
   pub exits: Vec<String>,
    pub dont_understand: bool,
}

fn is_message_title(line: &str) -> Result<String, Box<dyn Error>> {
    let re = Regex::new(r"== (?<title>.*) ==")?;
    let Some(capture) = re.captures(line) else {
        return Err("no match".into());
    };
    let title: String = capture["title"].to_string();
    Ok(title)
}
fn is_exit_title(line: &str) -> Result<u8, Box<dyn Error>> {
    let re = Regex::new(r"There .* (?<exits>[0-9]+) exit.*:")?;
    let Some(capture) = re.captures(line) else {
        return Err("no match".into());
    };
    let exits: u8 = capture["exits"].parse::<u8>()?;
    Ok(exits)
}

fn is_things_title(line: &str) -> bool {
    line.trim() == "Things of interest here:"
}
fn is_last_question_line(line: &str) -> bool {
    line.trim() == "What do you do?"
}
fn is_do_not_understand(line: &str) -> bool {
    line.trim() == "I don't understand; try 'help' for instructions."
}
fn is_slash_command(line: &str) -> bool {
    line.trim_start().starts_with("/")
}

fn is_item(line: &str) -> Result<String, Box<dyn Error>> {
    let re = Regex::new(r"^ *- (?<item>.*)$")?;
    let Some(capture) = re.captures(line) else {
        return Err("no match".into());
    };
    let item: String = capture["item"].to_string();
    Ok(item)
}

#[derive(Eq, PartialEq, Debug)]
enum MessageSections {
    Pretext,
    Message,
    Things,
    Exits,
    AfterPrompt,
    DoNotUnderstand,
}

impl<'a> OuputAnalyzer<'a> {
    pub fn new(response: &'a str) -> Self {
        OuputAnalyzer { response }
    }
    pub fn parse(&self) -> Result<ResponseParts, String> {
        let response_lines = self.response.lines();
        let mut parsed_lines = 0;
        let mut buffer = String::new();
        let mut pretext = String::new();
        let mut section: MessageSections = MessageSections::Pretext;
        let mut message_title = String::new();
        let mut message = String::new();
        let mut things = vec![];
        let mut exits = vec![];
        let mut exits_num = 0;
        let mut lines_it = response_lines.into_iter();
        let mut dont_understand = false;
        while let Some(line) = lines_it.next() {
            if let Ok(t) = is_message_title(line)
                && section == MessageSections::Pretext
            {
                eprintln!("got message title");
                trace!("encounter message title");
                section = MessageSections::Message;
                pretext.push_str(buffer.as_str().trim_end());
                buffer.clear();
                message_title.push_str(&t);
            } else if is_things_title(line) && section == MessageSections::Message {
                trace!("encounter things title");
                section = MessageSections::Things;
                message.push_str(buffer.trim_end());
                buffer.clear();
            } else if let Ok(exits) = is_exit_title(line)
                && (section == MessageSections::Things || section == MessageSections::Message)
            {
                eprintln!("got exit title");
                trace!("encounter exit title");
                exits_num = exits;
                match section {
                    MessageSections::Message => {
                        message.push_str(buffer.trim_end());
                        buffer.clear();
                    }
                    MessageSections::Things => {
                        assert!(
                            buffer.trim().is_empty(),
                            "buffer should be empty as 'things of interest' contains only items and no messages, but was {}", buffer
                        );
                    }
                    _ => {
                        assert!(
                            false,
                            "here no other sections, rather than Message or Things are expected, but was {:?}",
                            section
                        );
                    }
                }
                section = MessageSections::Exits;
            } else if is_last_question_line(line) {
                trace!("encounter last question line");
                section = MessageSections::AfterPrompt;
            } else if is_do_not_understand(line) {     
                trace!("encounter 'do not understand' line");
                section = MessageSections::DoNotUnderstand;
                dont_understand = true;
                if !buffer.trim().is_empty() {
                pretext.push_str(buffer.trim());
                buffer.clear();
                pretext.push('\n');
                }
                pretext.push_str(line.trim());
            } else if is_slash_command(line) {     
                // Do not store slash commands in analysis
                continue;
            } else {
                if let Ok(val) = is_item(line) {
                    match section {
                        MessageSections::Things => {
                            things.push(val);
                        }
                        MessageSections::Exits => {
                            exits.push(val);
                        }
                        MessageSections::Pretext => {
                            return Err("items should not encounter in pretext".into());
                        }
                        MessageSections::Message => {
                            debug!("message test is {}", self.response );
                            return Err("items should not encounter in message text".into());
                        }
                        MessageSections::AfterPrompt => {
                            return Err("cannot contain any text after the question prompt".into());
                        },
                        MessageSections::DoNotUnderstand => {
                            return Err("items should not encounter in error message".into());
                        }
                    }
                } else {
                    buffer.push_str(line);
                    buffer.push('\n');
                }
            }
            parsed_lines += 1;
        }
        assert_eq!(
            section,
            MessageSections::AfterPrompt,
            "message should end with the user question"
        );
        assert_eq!(
            exits_num as usize,
            exits.len(),
            "declared exits number must match the parsed exits number Exits: {:?}", exits
        );
        if parsed_lines == 0 {
            return Err("nothing was parsed".into());
        }
        Ok(ResponseParts {
            pretext,
            message,
            exits,
            dont_understand,
            things_of_interest: things,
            title: message_title,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exit_title_2() {
        let line = "There are 2 exits:";
        let res = is_exit_title(line);
        assert!(res.is_ok(), "{}", format!("res is  {:?}", res));
        assert_eq!(res.unwrap(), 2);
    }
    #[test]
    fn test_exit_title_1_space() {
        let line = " There is 1 exit: ";
        let res = is_exit_title(line);
        assert!(res.is_ok(), "{}", format!("res is  {:?}", res));
        assert_eq!(res.unwrap(), 1);
    }
    #[test]
    fn test_exit_title_1() {
        let line = "There is 1 exit:";
        let res = is_exit_title(line);
        assert!(res.is_ok(), "{}", format!("res is  {:?}", res));
        assert_eq!(res.unwrap(), 1);
    }
    #[test]
    fn test_exit_title_100() {
        let line = "There are 100 exits:";
        let res = is_exit_title(line);
        assert!(res.is_ok(), "{}", format!("res is  {:?}", res));
        assert_eq!(res.unwrap(), 100);
    }
    #[test]
    fn test_exit_title_no_match() {
        let line = "There is something else";
        let res = is_exit_title(line);
        assert!(res.is_err());
    }
    #[test]
    fn test_is_item() {
        let line = "- south";
        let res = is_item(line);
        assert!(res.is_ok());
        assert_eq!(res.unwrap(),"south")
    }
    #[test]
    fn test_is_item_space() {
        let line = "    - north";
        let res = is_item(line);
        assert!(res.is_ok());
        assert_eq!(res.unwrap(),"north")
    }

    #[test]
    fn test_some_abstract() {
        let paragraph = r#"
== Twisty passages ==
You are in a twisty maze of little passages, all alike.

There are 3 exits:
- north
- south
- west

What do you do?
"#;
        let op = OuputAnalyzer::new(paragraph);
        match op.parse() {
            Ok(result) => {
                assert_eq!(result.title, "Twisty passages");
                assert_eq!(result.exits.len(), 3);
                assert!(result.pretext.is_empty());
                assert!(result.things_of_interest.is_empty());
                assert_eq!(
                    result.message, "You are in a twisty maze of little passages, all alike.",
                    "Parsed object is {:?}",
                    result
                );
            }
            Err(parse_err) => {
                assert!(false, "failed to parse message. Error: {}", parse_err);
            }
        }
    }
    #[test]
    fn test_initial_paragraph() {
        let paragraph = r#"Welcome to the Synacor Challenge!
Please record your progress by putting codes like
this one into the challenge website: uxlzSuIDThsw

Executing self-test...

self-test complete, all tests pass
The self-test completion code is: jGxkvqlwrGNE

== Foothills ==
You find yourself standing at the base of an enormous mountain.  At its base to the north, there is a massive doorway.  A sign nearby reads "Keep out!  Definitely no treasure within!"

Things of interest here:
- tablet

There are 2 exits:
- doorway
- south

What do you do?
"#;
        let op = OuputAnalyzer::new(paragraph);
        match op.parse() {
            Ok(result) => {
                assert_eq!(result.title, "Foothills");
                assert_eq!(result.exits.len(), 2);
                assert_eq!(result.pretext, r#"Welcome to the Synacor Challenge!
Please record your progress by putting codes like
this one into the challenge website: uxlzSuIDThsw

Executing self-test...

self-test complete, all tests pass
The self-test completion code is: jGxkvqlwrGNE"#);
                assert_eq!(result.things_of_interest.len(), 1);
                assert_eq!(
                    result.message, "You find yourself standing at the base of an enormous mountain.  At its base to the north, there is a massive doorway.  A sign nearby reads \"Keep out!  Definitely no treasure within!\"",
                    "Parsed object is {:?}",
                    result
                );
            }
            Err(parse_err) => {
                assert!(false, "failed to parse message. Error: {}", parse_err);
            }
        }
    }
    #[test]
    fn test_initial_small_paragraph() {
        let paragraph = r#"
    == Foothills ==
    As you begin to leave, you feel the urge for adventure pulling you back...

    There is 1 exit:
    - north

    What do you do?
"#;
        let op = OuputAnalyzer::new(paragraph);
        match op.parse() {
            Ok(result) => {
                assert_eq!(result.title, "Foothills");
                assert_eq!(result.exits.len(), 1);
                assert!(result.pretext.is_empty());
                assert_eq!(result.things_of_interest.len(), 0);
                assert_eq!(
                    result.message, "    As you begin to leave, you feel the urge for adventure pulling you back...",
                    "Parsed object is {:?}",
                    result
                );
            }
            Err(parse_err) => {
                assert!(false, "failed to parse message. Error: {}", parse_err);
            }
        }
    }
    #[test]
    fn test_initial_malformed_paragraph() {
        let paragraph = r#"
    == Foothills ==
    As you begin to leave, you feel the urge for adventure pulling you back...

    There is 1 exit:
    - north

    What do you do?
    /show_state

    I don't understand; try 'help' for instructions.

    What do you do?
    /show_state
"#;
        let op = OuputAnalyzer::new(paragraph);
        match op.parse() {
            Ok(result) => {
                assert_eq!(result.title, "Foothills");
                assert_eq!(result.exits.len(), 1);
                assert_eq!(result.pretext, "I don't understand; try 'help' for instructions.");
                assert_eq!(result.things_of_interest.len(), 0);
                assert_eq!(
                    result.message, "    As you begin to leave, you feel the urge for adventure pulling you back...", "Parsed object is {:?}",
                    result
                );
            }
            Err(parse_err) => {
                assert!(false, "failed to parse message. Error: {}", parse_err);
            }
        }
    }

    #[test]
    fn test_do_not_understand() {
        let paragraph = r#"
    I don't understand; try 'help' for instructions.

    What do you do?
"#;
        let op = OuputAnalyzer::new(paragraph);
        match op.parse() {
            Ok(result) => {
                assert!(result.dont_understand);
                assert_eq!( result.pretext, "I don't understand; try 'help' for instructions.", "Parsed object is {:?}", result);
                assert_eq!(result.title, "");
                assert_eq!(result.exits.len(), 0);
                assert_eq!(result.things_of_interest.len(), 0);
                assert!( result.message.is_empty(), "Parsed object is {:?}", result);
            }
            Err(parse_err) => {
                assert!(false, "failed to parse message. Error: {}", parse_err);
            }
        }
    }
    // TODO: write tests for this output
    /* 
 == Foothills ==
You find yourself standing at the base of an enormous mountain.  At its base to the north, there is a massive doorway.  A sign nearby reads "Keep out!  Definitely no treasure within!"

Things of interest here:
- tablet

There are 2 exits:
- doorway
- south

What do you do?
take tablet
got message title
got exit title


Taken.

What do you do?
look tablet


The tablet seems appropriate for use as a writing surface but is unfortunately blank.  Perhaps you should USE it as a writing surface...

What do you do?
use tablet


You find yourself writing "QDcZQJqVCzKL" on the tablet.  Perhaps it's some kind of code?


What do you do?
go doorway


== Dark cave ==
This seems to be the mouth of a deep cave.  As you peer north into the darkness, you think you hear the echoes of bats deeper within.

There are 2 exits:
- north
- south

What do you do?
*/
}
