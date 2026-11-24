//! SuperLink: UTF-8 preserving positional substitution for DM's OPE fallback.
//! SQL literals are already encoded by the typed parameter encoder.
use crate::{Error,Result};
pub fn substitute(sql:&str,literals:&[String])->Result<String>{
    let mut chars=sql.chars().peekable();let mut result=String::with_capacity(sql.len());
    let mut quote=None;let mut line=false;let mut block=0usize;let mut parameter=0;
    while let Some(ch)=chars.next(){
        if result.len()>32<<20{return Err(Error::QueryFailed("bound statement exceeds 32 MiB".into()));}
        if line{result.push(ch);if ch=='\n'{line=false;}continue;}
        if block>0{result.push(ch);if ch=='/'&&chars.peek()==Some(&'*'){result.push(chars.next().unwrap());block+=1;}else if ch=='*'&&chars.peek()==Some(&'/'){result.push(chars.next().unwrap());block-=1;}continue;}
        if let Some(end)=quote{
            result.push(ch);
            if ch==end{if chars.peek()==Some(&end){result.push(chars.next().unwrap());}else{quote=None;}}
            else if ch=='\\'&&end=='\''{if let Some(escaped)=chars.next(){result.push(escaped);}}
            continue;
        }
        match ch{
            '\''|'"'|'`'=>{quote=Some(ch);result.push(ch);},
            '['=>{quote=Some(']');result.push(ch);},
            '-' if chars.peek()==Some(&'-')=>{line=true;result.push(ch);result.push(chars.next().unwrap());},
            '/' if chars.peek()==Some(&'*')=>{block=1;result.push(ch);result.push(chars.next().unwrap());},
            '?'=>{let literal=literals.get(parameter).ok_or_else(||Error::QueryFailed("missing positional parameter".into()))?;result.push_str(literal);parameter+=1;},
            _=>result.push(ch),
        }
    }
    if parameter!=literals.len(){return Err(Error::QueryFailed("positional parameter count mismatch".into()));}
    Ok(result)
}
