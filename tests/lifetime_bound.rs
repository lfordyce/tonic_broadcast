use tokio;
use tokio::macros::support::Future;

#[tokio::test]
async fn trait_bound() {
    wrapper(add_one).await;

    foo(|num_iter| Box::new(num_iter.map(|i| *i)));
}

trait AsyncFnMutArg<'a, P: 'a, T> {
    type Fut: Future<Output=T> + 'a;
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
    *i = *i + 1;
}

pub fn foo<Func>(closure: Func)
    where
            for<'a> Func: Fn(std::slice::Iter<'a, u32>) -> Box<dyn Iterator<Item=u32> + 'a>,
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

trait SumRankSlice<R> where Self: Rank<R> + Sized {
    fn sum(slice: &[Self]) -> u32 {
        slice.iter().map(Self::rank).sum()
    }
}

/// Tautology: slice-summable rankable data is rankable data that can be slice-summed.
impl<R, T> SumRankSlice<R> for T where T: Rank<R> {}

impl Rank<DefaultRanker> for i32 {
    fn rank(&self) -> u32 {
        if *self >= 0 { *self as u32 } else { 0 }
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
