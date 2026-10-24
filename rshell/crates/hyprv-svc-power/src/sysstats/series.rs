//! Inline, ordered bounded samples; cloning a published snapshot never allocates.
use serde::{Deserialize, Deserializer, Serialize, Serializer};
#[derive(Clone, Debug)]
pub struct Series<const N: usize, const TAIL: bool = true> {
    values: [Option<f64>; N],
    start: usize,
    len: usize,
}
impl<const N: usize, const TAIL: bool> Default for Series<N, TAIL> {
    fn default() -> Self {
        Self {
            values: [None; N],
            start: 0,
            len: 0,
        }
    }
}
impl<const N: usize, const TAIL: bool> Series<N, TAIL> {
    pub fn len(&self) -> usize {
        self.len
    }
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    pub fn clear(&mut self) {
        self.start = 0;
        self.len = 0;
    }
    pub fn push_back(&mut self, v: Option<f64>) {
        if N == 0 {
            return;
        }
        if self.len == N {
            if !TAIL {
                return;
            }
            self.values[self.start] = v;
            self.start = (self.start + 1) % N;
        } else {
            self.values[(self.start + self.len) % N] = v;
            self.len += 1;
        }
    }
    pub fn pop_front(&mut self) -> Option<Option<f64>> {
        if self.len == 0 {
            return None;
        }
        let v = self.values[self.start];
        self.start = (self.start + 1) % N;
        self.len -= 1;
        Some(v)
    }
    pub fn iter(&self) -> impl ExactSizeIterator<Item = &Option<f64>> + DoubleEndedIterator {
        (0..self.len).map(move |i| &self.values[(self.start + i) % N])
    }
}
impl<const N: usize, const TAIL: bool> PartialEq for Series<N, TAIL> {
    fn eq(&self, other: &Self) -> bool {
        self.len == other.len && self.iter().eq(other.iter())
    }
}
impl<const N: usize, const TAIL: bool> Serialize for Series<N, TAIL> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_seq(self.iter())
    }
}
impl<'de, const N: usize, const TAIL: bool> Deserialize<'de> for Series<N, TAIL> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V<const N: usize, const TAIL: bool>;
        impl<'de, const N: usize, const TAIL: bool> serde::de::Visitor<'de> for V<N, TAIL> {
            type Value = Series<N, TAIL>;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("bounded sample series")
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut a: A,
            ) -> Result<Self::Value, A::Error> {
                let mut out = Series::default();
                while let Some(v) = a.next_element()? {
                    out.push_back(v)
                }
                Ok(out)
            }
        }
        d.deserialize_seq(V::<N, TAIL>)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn core_wire_keeps_the_first_eight() {
        let series: Series<8, false> = serde_json::from_str("[0,1,2,3,4,5,6,7,8,9]").unwrap();
        assert_eq!(
            series.iter().copied().collect::<Vec<_>>(),
            (0..8).map(|v| Some(v as f64)).collect::<Vec<_>>()
        );
    }
    #[test]
    fn logical_equality_and_wire() {
        let mut a = Series::<3>::default();
        for _ in 0..4 {
            a.push_back(Some(1.));
        }
        let mut b = Series::<3>::default();
        for _ in 0..3 {
            b.push_back(Some(1.));
        }
        assert_eq!(a, b);
        assert_eq!(serde_json::to_string(&a).unwrap(), "[1.0,1.0,1.0]");
        assert_eq!(serde_json::from_str::<Series<3>>("[1,1,1]").unwrap(), a);
    }
}
