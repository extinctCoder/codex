use lax_domain::contracts::application::views::QueryParser;
use lax_shared::{
    dtos::db::mason::{MasonDirection, MasonFilter, MasonSort, MasonValue},
    error::{ApplicationError, LaxResult},
};
use nom::{
    IResult, Parser,
    branch::alt,
    bytes::complete::{tag, take_while1},
    character::complete::char,
    combinator::map,
    multi::separated_list1,
    sequence::delimited,
};

// Hard cap on parenthesised group nesting in RSQL expressions. Bounds parser
// recursion so a malicious or pathological filter cannot exhaust the stack.
const MAX_NESTING_DEPTH: usize = 10;

#[derive(Debug)]
pub struct RsqlQueryParser;

impl QueryParser for RsqlQueryParser {
    fn parse_filter(&self, input: &str) -> LaxResult<MasonFilter> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Err(ApplicationError::validation("RSQL expression is empty"));
        }
        match parse_or(trimmed, 0) {
            Ok(("", node)) => Ok(node),
            Ok((remaining, _)) => Err(ApplicationError::validation(format!(
                "unexpected trailing input in RSQL expression: '{remaining}'"
            ))),
            Err(_) => Err(ApplicationError::validation(format!(
                "invalid RSQL expression: '{trimmed}'"
            ))),
        }
    }

    fn parse_sort(&self, input: &str) -> LaxResult<Vec<MasonSort>> {
        let clauses = input
            .split(',')
            .map(str::trim)
            .filter(|entry| !entry.is_empty())
            .map(|entry| {
                if let Some(column) = entry.strip_prefix('-') {
                    MasonSort {
                        column: column.trim().to_string(),
                        direction: MasonDirection::Desc,
                    }
                } else if let Some(column) = entry.strip_prefix('+') {
                    MasonSort {
                        column: column.trim().to_string(),
                        direction: MasonDirection::Asc,
                    }
                } else {
                    MasonSort {
                        column: entry.to_string(),
                        direction: MasonDirection::Asc,
                    }
                }
            })
            .collect();
        Ok(clauses)
    }
}

// ── RSQL parser helpers ────────────────────────────────────────────────────

fn parse_or(input: &str, depth: usize) -> IResult<&str, MasonFilter> {
    let (input, first) = parse_and(input, depth)?;
    let (input, rest) = nom::multi::many0(|remaining| {
        let (remaining, _) = char(',').parse(remaining)?;
        parse_and(remaining, depth)
    })
    .parse(input)?;

    if rest.is_empty() {
        Ok((input, first))
    } else {
        let mut children = vec![first];
        children.extend(rest);
        Ok((input, MasonFilter::Or(children)))
    }
}

fn parse_and(input: &str, depth: usize) -> IResult<&str, MasonFilter> {
    let (input, first) = parse_primary(input, depth)?;
    let (input, rest) = nom::multi::many0(|remaining| {
        let (remaining, _) = char(';').parse(remaining)?;
        parse_primary(remaining, depth)
    })
    .parse(input)?;

    if rest.is_empty() {
        Ok((input, first))
    } else {
        let mut children = vec![first];
        children.extend(rest);
        Ok((input, MasonFilter::And(children)))
    }
}

fn parse_primary(input: &str, depth: usize) -> IResult<&str, MasonFilter> {
    if input.starts_with('(') {
        parse_group(input, depth)
    } else {
        parse_comparison(input)
    }
}

fn parse_group(input: &str, depth: usize) -> IResult<&str, MasonFilter> {
    if depth >= MAX_NESTING_DEPTH {
        return Err(nom::Err::Failure(nom::error::Error::new(
            input,
            nom::error::ErrorKind::TooLarge,
        )));
    }
    delimited(char('('), |inner| parse_or(inner, depth + 1), char(')')).parse(input)
}

fn parse_comparison(input: &str) -> IResult<&str, MasonFilter> {
    let (input, column) = parse_identifier(input)?;
    let (input, operator) = parse_operator(input)?;
    match operator {
        "==" => {
            let (input, value) = parse_single(input)?;
            Ok((input, MasonFilter::Eq(column.to_string(), MasonValue::Text(value))))
        }
        "!=" => {
            let (input, value) = parse_single(input)?;
            Ok((input, MasonFilter::NotEq(column.to_string(), MasonValue::Text(value))))
        }
        "=gt=" => {
            let (input, value) = parse_single(input)?;
            Ok((input, MasonFilter::Gt(column.to_string(), MasonValue::Text(value))))
        }
        "=ge=" => {
            let (input, value) = parse_single(input)?;
            Ok((input, MasonFilter::Gte(column.to_string(), MasonValue::Text(value))))
        }
        "=lt=" => {
            let (input, value) = parse_single(input)?;
            Ok((input, MasonFilter::Lt(column.to_string(), MasonValue::Text(value))))
        }
        "=le=" => {
            let (input, value) = parse_single(input)?;
            Ok((input, MasonFilter::Lte(column.to_string(), MasonValue::Text(value))))
        }
        "=like=" => {
            let (input, value) = parse_single(input)?;
            Ok((input, MasonFilter::Like(column.to_string(), value)))
        }
        "=starts=" => {
            let (input, value) = parse_single(input)?;
            Ok((input, MasonFilter::StartsWith(column.to_string(), value)))
        }
        "=ends=" => {
            let (input, value) = parse_single(input)?;
            Ok((input, MasonFilter::EndsWith(column.to_string(), value)))
        }
        "=in=" => {
            let (input, values) = parse_list(input)?;
            let mason_values = values.into_iter().map(MasonValue::Text).collect();
            Ok((input, MasonFilter::In(column.to_string(), mason_values)))
        }
        "=out=" => {
            let (input, values) = parse_list(input)?;
            let mason_values = values.into_iter().map(MasonValue::Text).collect();
            Ok((input, MasonFilter::NotIn(column.to_string(), mason_values)))
        }
        "=null=" => {
            let (input, boolean) = parse_bool(input)?;
            Ok((input, MasonFilter::IsNull(column.to_string(), boolean)))
        }
        _ => unreachable!("parse_operator returned an unknown token"),
    }
}

fn parse_identifier(input: &str) -> IResult<&str, &str> {
    take_while1(|character: char| character.is_alphanumeric() || character == '_' || character == '.').parse(input)
}

fn parse_operator(input: &str) -> IResult<&str, &str> {
    alt((
        tag("=="),
        tag("!="),
        tag("=gt="),
        tag("=ge="),
        tag("=lt="),
        tag("=le="),
        tag("=like="),
        tag("=starts="),
        tag("=ends="),
        tag("=in="),
        tag("=out="),
        tag("=null="),
    ))
    .parse(input)
}

fn parse_single(input: &str) -> IResult<&str, String> {
    let (input, raw) = take_while1(|character: char| !matches!(character, ';' | ',' | '(' | ')')).parse(input)?;
    Ok((input, raw.to_string()))
}

fn parse_list(input: &str) -> IResult<&str, Vec<String>> {
    let (input, items) = delimited(
        char('('),
        separated_list1(
            char(','),
            map(
                take_while1(|character: char| !matches!(character, ',' | ')')),
                str::to_string,
            ),
        ),
        char(')'),
    )
    .parse(input)?;
    Ok((input, items))
}

fn parse_bool(input: &str) -> IResult<&str, bool> {
    alt((map(tag("true"), |_| true), map(tag("false"), |_| false))).parse(input)
}
