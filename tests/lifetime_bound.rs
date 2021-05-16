use num;
use num::cast::AsPrimitive;
use num::FromPrimitive;
use std::collections::HashMap;
use std::convert::TryFrom;
use std::fmt::Formatter;
use std::num::ParseIntError;
use std::str::FromStr;
use std::sync::Mutex;
use tokio;
use tokio::macros::support::Future;

fn pi<T: 'static + Copy>() -> T
where
    f64: num::cast::AsPrimitive<T>,
{
    let value: T = (3.14159265358979323846264338327950288419716939937510f64).as_();
    value
}

fn screen_width<T: 'static + Copy>() -> T
where
    T: num::cast::FromPrimitive,
{
    let value: Option<T> = FromPrimitive::from_u16(2047u16);
    value.expect("Type does not support value 2047")
}

#[tokio::test]
async fn trait_bound() {
    wrapper(add_one).await;

    foo(|num_iter| Box::new(num_iter.map(|i| *i)));
}

trait AsyncFnMutArg<'a, P: 'a, T> {
    type Fut: Future<Output = T> + 'a;
    fn call(self, arg: &'a mut P) -> Self::Fut;
}

impl<'a, P: 'a, Fut: Future + 'a, F: FnOnce(&'a mut P) -> Fut> AsyncFnMutArg<'a, P, Fut::Output>
    for F
{
    type Fut = Fut;
    fn call(self, arg: &'a mut P) -> Self::Fut {
        self(arg)
    }
}

async fn wrapper<F>(f: F)
where
    F: for<'a> AsyncFnMutArg<'a, i32, ()>,
{
    let mut i = 41;
    f.call(&mut i).await;
}

async fn add_one(i: &mut i32) {
    *i += 1;
}

pub fn foo<Func>(closure: Func)
where
    for<'a> Func: Fn(std::slice::Iter<'a, u32>) -> Box<dyn Iterator<Item = u32> + 'a>,
{
    let nums = vec![1, 2, 3];

    for num in closure(nums.iter()) {
        println!("{}", num);
    }
}

/// A trait of rankable data
trait Rank<R> {
    fn rank(&self) -> u32;
}

/// Marker for default rank implementations.
struct DefaultRanker;

trait SumRankSlice<R>
where
    Self: Rank<R> + Sized,
{
    fn sum(slice: &[Self]) -> u32 {
        slice.iter().map(Self::rank).sum()
    }
}

/// Tautology: slice-summable rankable data is rankable data that can be slice-summed.
impl<R, T> SumRankSlice<R> for T where T: Rank<R> {}

impl Rank<DefaultRanker> for i32 {
    fn rank(&self) -> u32 {
        if *self >= 0 {
            *self as u32
        } else {
            0
        }
    }
}

#[tokio::test]
async fn test_i32_default() {
    assert_eq!(6, SumRankSlice::<DefaultRanker>::sum(&[1, 2, 3]));
}

struct HigherRanker;

impl Rank<HigherRanker> for i32 {
    fn rank(&self) -> u32 {
        Rank::<DefaultRanker>::rank(self) + 5
    }
}

#[test]
fn test_i32_higher() {
    assert_eq!(21, SumRankSlice::<HigherRanker>::sum(&[1, 2, 3]));
}

#[derive(PartialEq, Eq)]
enum Progress {
    None,
    Some,
    Complete,
}

fn count_iterator(map: &HashMap<String, Progress>, value: Progress) -> usize {
    map.values().filter(|&v| *v == value).count()
}

fn count_collection_iter(collection: &[HashMap<String, Progress>], value: Progress) -> usize {
    collection.iter().fold(0, |acc, x| {
        acc + x.values().filter(|&v| *v == value).count()
    })
}

pub fn average(values: &[f64]) -> f64 {
    let total = values.iter().fold(0.0, |a, b| a + b);
    total / values.len() as f64
}

#[derive(Debug)]
struct Person {
    name: String,
    age: usize,
}

#[derive(Debug)]
struct PersonError;

impl std::fmt::Display for PersonError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid red green blue values")
    }
}

impl std::error::Error for PersonError {}

impl FromStr for Person {
    type Err = Box<dyn std::error::Error>;
    fn from_str(s: &str) -> Result<Person, Self::Err> {
        Err(Box::new(PersonError))
    }
}

impl Default for Person {
    fn default() -> Self {
        Self {
            name: String::from("John"),
            age: 30,
        }
    }
}

impl From<&str> for Person {
    fn from(s: &str) -> Self {
        if s.is_empty() {
            Self::default()
        } else {
            let v: Vec<&str> = s.split(',').collect();
            match v.len() {
                2 => {
                    let name = match v.get(0) {
                        None => return Person::default(),
                        Some(name) => {
                            if name.is_empty() {
                                return Person::default();
                            } else {
                                name
                            }
                        }
                    };
                    let age = match v.get(1) {
                        None => return Person::default(),
                        Some(age) => match age.parse::<usize>() {
                            Ok(age) => age,
                            Err(_) => return Person::default(),
                        },
                    };
                    Person {
                        name: String::from(*name),
                        age,
                    }
                }
                _ => Person::default(),
            }
        }
    }
}

#[derive(Debug, PartialEq)]
struct Color {
    red: u8,
    green: u8,
    blue: u8,
}

#[derive(Debug)]
struct ColorError;

impl std::fmt::Display for ColorError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid red green blue values")
    }
}

impl std::error::Error for ColorError {}

impl TryFrom<(i16, i16, i16)> for Color {
    type Error = Box<dyn std::error::Error>;

    fn try_from(tuple: (i16, i16, i16)) -> Result<Self, Self::Error> {
        match tuple {
            (0..=255, 0..=255, 0..=255) => Ok(Color {
                red: tuple.0 as u8,
                green: tuple.1 as u8,
                blue: tuple.2 as u8,
            }),
            _ => Err(Box::new(ColorError)),
        }
    }
}

impl TryFrom<[i16; 3]> for Color {
    type Error = Box<dyn std::error::Error>;
    fn try_from(arr: [i16; 3]) -> Result<Self, Self::Error> {
        let color_tuple: (i16, i16, i16) = (arr[0], arr[1], arr[2]);
        match color_tuple {
            (0..=255, 0..=255, 0..=255) => Ok(Color {
                red: color_tuple.0 as u8,
                green: color_tuple.1 as u8,
                blue: color_tuple.2 as u8,
            }),
            _ => Err(Box::new(ColorError)),
        }
    }
}

fn byte_counter<T: AsRef<str>>(arg: T) -> usize {
    arg.as_ref().as_bytes().len()
}

#[test]
fn test_primitive() {
    println!("{}", pi::<f32>());
    println!("{}", pi::<f64>());
    println!("{}", screen_width::<u16>());
    println!("{}", screen_width::<f32>());
    println!("{}", screen_width::<i64>());
    // will fail with a panic
    //println!("{}", screen_width::<i8>());
}

#[test]
fn test_missing_name_and_invalid_age() {
    let p: Person = Person::from("Mark");
    println!("{:?}", p.name);
    println!("{:?}", p.age);
    assert_eq!(p.name, "John");
}
