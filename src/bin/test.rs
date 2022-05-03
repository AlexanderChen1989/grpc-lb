fn main() {
    use rand::distributions::WeightedIndex;
    use rand::prelude::*;

    let choices = ['a', 'b', 'c'];
    let weights = [2, 1, 1];
    let dist = WeightedIndex::new(&weights).unwrap();
    let mut rng = thread_rng();
    for _ in 0..100 {
        println!("{}", choices[dist.sample(&mut rng)]);
    }
}
