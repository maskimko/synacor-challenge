use regex::Regex;
use std::error::Error;
struct OuputAnalyzer<'a> {
    response_lines: &'a[&'a str],
}

struct ResponseParts {
    title: String,
    message: String,
    commands: Vec<String>,

}

fn is_exit_title(line: &str) -> Result<u8,Box<dyn Error>> {
    let re = Regex::new(r"There are (?<exits>[0-9]+) exit.*:")?;
    let Some(capture) = re.captures(line) else {
        return Err("no match".into());
    };
        let exits : u8= capture["exits"].parse::<u8>()?;
        Ok(exits)
}

impl<'a> OuputAnalyzer<'_> {
    fn parse(&self) -> Result<ResponseParts, String> {
        if self.response_lines.is_empty() {
            return Err("response is empty".into());
        }
        let mut lines_it  = self.response_lines.into_iter();

    let title = lines_it.next().expect("if reponse is not empty, there must be at least 1 line, which is considered as title"); // First line is title
        while let Some(line) = lines_it.next() {
            unimplemented!();
        }
         
unimplemented!();
}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exit_title_2() {
        let line = "There are 2 exits:";
        let res = is_exit_title(line);
        assert!(res.is_ok(), "{}", format!("res is  {:?}",res));
        assert_eq!(res.unwrap(), 2);

    }
    #[test]
    fn test_exit_title_1() {
        let line = "There are 1 exit:";
        let res = is_exit_title(line);
        assert!(res.is_ok(), "{}", format!("res is  {:?}",res));
        assert_eq!(res.unwrap(), 1);

    }
    #[test]
    fn test_exit_title_100() {
        let line = "There are 100 exits:";
        let res = is_exit_title(line);
        assert!(res.is_ok(), "{}", format!("res is  {:?}",res));
        assert_eq!(res.unwrap(), 100);

    }
    #[test]
    fn test_exit_title_no_match() {
        let line = "There is something else";
        let res = is_exit_title(line);
        assert!(res.is_err());
    }
}
