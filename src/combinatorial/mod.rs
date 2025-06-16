use savefile_derive::Savefile;
use serde::{Deserialize, Serialize};

/// The Combinatorial Encoder uses a combinatorial number system to uniquely identify a particular
/// combination of K elements out of a set of N possibilities using a single integer. This mapping
/// is contiguous, one-to-one, and preserves the lexicographical order of combinations. That is, if
/// `A` and `B` are two combinations and `A < B` (lexicographically), then `encode(A) < encode(B)`.
///
/// Let $i = 0, ..., N-1$ represent the elements of the set of N possibilities. For a given
/// combination of $k$ elements, let $0 <= c_1 < c_2 < ... < c_k$. Then the combinatorial index is
/// given by the following integer $N$:
/// $$ N = nCr(c_k, k) + ... + nCr(c_1, 1) $$
/// where $nCr$ is the notation for "n choose r". This encoding works for a particular, fixed value
/// of $k$. To handle combinations of up to length `MAX_K`, the encoding is offset so that lower
/// values of $k$ precede larger values of $k$. E.g., the encoding for ${}$ (i.e., `nCr(N, 0)`) is
/// 0, ${0}, {1}, ..., {N-1}$ are mapped to $1, ..., N$, and ${0, 1}, {0, 2}, ...$ are mapped to
/// $N+1, ..., N + nCr(N, 2)$.
///
/// We assume that the combination is represented as bitflags within a `u64` and provide methods for
/// _encoding_ (combination -> index) and _decoding_ (index -> combination).
#[derive(Savefile, Serialize, Deserialize)]
pub struct CombinatorialEncoder<const N: u8, const MAX_K: u8> {
    /// Binomial coefficients (i.e., Pascal's triangle) stored in column-major order
    binom: Vec<u32>,
    /// Offsets for combinations of length $k < MAX_K$.
    size_offsets: Vec<u32>,
}

/// Computes the index into the column-major flat array for the given row/column of pascal's triangle.
fn triangle_index(row: u8, column: u8, n: u8) -> usize {
    column as usize * (2 * (n as usize) - (column as usize) + 1) / 2 + (row as usize)
}

fn binomial_coeff(triangle: &[u32], row: u8, column: u8, n: u8) -> u32 {
    if row < column {
        0
    } else {
        triangle[triangle_index(row, column, n)]
    }
}

impl<const N: u8, const MAX_K: u8> Default for CombinatorialEncoder<N, MAX_K> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: u8, const MAX_K: u8> CombinatorialEncoder<N, MAX_K> {
    pub fn new() -> Self {
        let n = N as usize;
        let mut binom = vec![0u32; (n + 1) * (n + 2) / 2];

        // Build pascal's triangle using the relationship: (n choose k) = (n-1 choose k-1) + (n-1 choose k)
        // Add the (0,0)th entry.
        binom[0] = 1;

        for row in 1..=N {
            for column in 0..=row {
                binom[triangle_index(row, column, N)] = if column == 0 || column == row {
                    1
                } else {
                    binomial_coeff(&binom, row - 1, column - 1, N)
                        + binomial_coeff(&binom, row - 1, column, N)
                };
            }
        }

        let mut size_offsets = vec![0; (MAX_K + 2) as usize];
        let mut running_total = 0u32;
        for k in 0..MAX_K + 1 {
            size_offsets[k as usize] = running_total;
            running_total += binom[triangle_index(N, k, N)];
        }
        // Record maximum value as well
        size_offsets[MAX_K as usize + 1] = running_total;

        Self {
            binom,
            size_offsets,
        }
    }

    /// Encodes a combination (represented as a bitset) as an integer.
    pub fn encode(&self, bitset: u64) -> u32 {
        let k = bitset.count_ones() as usize;
        assert!(
            k <= MAX_K as usize,
            "can only encode up to MAX_K items in a combination"
        );

        let mut local_idx = 0;
        let mut remaining = bitset;
        let mut counter = 1;
        while remaining > 0 {
            let elem = remaining.trailing_zeros();
            // Zero out the last bit
            remaining &= !(1 << elem);
            local_idx += if elem < counter as u32 {
                0
            } else {
                self.binom[triangle_index(elem as u8, counter, N)]
            };
            counter += 1;
        }

        self.size_offsets[k] + local_idx
    }

    /// Decodes an integer into a combination.
    pub fn decode(&self, index: u32) -> u64 {
        let mut k = self
            .size_offsets
            .partition_point(|x| *x <= index)
            .saturating_sub(1) as u8;

        let mut bitset = 0;

        let mut local_idx = index - self.size_offsets[k as usize];
        while k > 0 {
            let mut elem = N;
            let mut value = self.binom[triangle_index(elem, k, N)];
            while value > local_idx {
                elem -= 1;
                value = if elem < k {
                    0
                } else {
                    self.binom[triangle_index(elem, k, N)]
                };
            }
            bitset |= 1 << elem;
            local_idx -= value;
            k -= 1
        }

        bitset
    }

    pub fn maximum_index(&self) -> u32 {
        self.size_offsets[(MAX_K + 1) as usize]
    }
}

#[cfg(test)]
mod tests {
    use crate::combinatorial::{triangle_index, CombinatorialEncoder};

    #[test]
    fn test_triangle_index() {
        // We want to build Pascal's triangle and then store it in column-major order.
        //          1
        //         1 1
        //        1 2 1
        //       1 3 3 1
        //
        // gets mapped to [1, 1, 1, 1, 1, 2, 3, 1, 3, 1]. Thus, the index in a row-major traversal
        // is [0,
        // 1, 4,
        // 2, 5, 7,
        // 3, 6, 8, 9].
        let expected = [0, 1, 4, 2, 5, 7, 3, 6, 8, 9];
        let mut idx = 0;
        for row in 0..4 {
            for column in 0..=row {
                let actual = triangle_index(row, column, 3);
                let expected = expected[idx];
                assert_eq!(
                    actual, expected,
                    "index({row}, {column}) = {actual} != {expected}"
                );
                idx += 1;
            }
        }
    }

    #[test]
    fn test_five_rows() {
        let encoder = CombinatorialEncoder::<5, 5>::new();
        // Below is the row-major ordering
        // let expected = vec![
        //     1, // row 0
        //     1, 1, // row 1
        //     1, 2, 1, // row 2
        //     1, 3, 3, 1, // row 3
        //     1, 4, 6, 4, 1, // row 4
        //     1, 5, 10, 10, 5, 1, // row 5
        // ];
        let expected = [
            1, 1, 1, 1, 1, 1, 1, 2, 3, 4, 5, 1, 3, 6, 10, 1, 4, 10, 1, 5, 1,
        ];
        assert_eq!(encoder.binom, expected);
        assert_eq!(encoder.size_offsets, [0, 1, 6, 16, 26, 31, 32]);
    }

    #[test]
    fn test_five_four() {
        let encoder = CombinatorialEncoder::<5, 4>::new();
        // Below is the row-major order
        // let expected = vec![
        //     1, // row 0
        //     1, 1, // row 1
        //     1, 2, 1, // row 2
        //     1, 3, 3, 1, // row 3
        //     1, 4, 6, 4, 1, // row 4
        //     1, 5, 10, 10, 5, 1, // row 5
        // ];
        let expected = [
            1, 1, 1, 1, 1, 1, 1, 2, 3, 4, 5, 1, 3, 6, 10, 1, 4, 10, 1, 5, 1,
        ];
        assert_eq!(encoder.binom, expected);
        assert_eq!(encoder.size_offsets, vec![0, 1, 6, 16, 26, 31]);
    }

    #[test]
    fn test_four_two() {
        let encoder = CombinatorialEncoder::<4, 2>::new();
        // Below is the row-major order
        // let expected = vec![
        //     1, // row 0
        //     1, 1, // row 1
        //     1, 2, 1, // row 2
        //     1, 3, 3, 1, // row 3
        //     1, 4, 6, 4, 1, // row 4
        // ];
        let expected = [1, 1, 1, 1, 1, 1, 2, 3, 4, 1, 3, 6, 1, 4, 1];
        assert_eq!(encoder.binom, expected);
        assert_eq!(encoder.size_offsets, vec![0, 1, 5, 11]);
    }

    #[test]
    fn test_roundtrip() {
        let encoder = CombinatorialEncoder::<4, 4>::new();
        let items = [
            0,                                 // 4 choose 0
            1 << 0,                            // 4 choose 1
            1 << 1,                            // 4 choose 1
            1 << 2,                            // 4 choose 1
            1 << 3,                            // 4 choose 1
            1 << 1 | 1 << 0,                   // 4 choose 2
            1 << 2 | 1 << 0,                   // 4 choose 2
            1 << 2 | 1 << 1,                   // 4 choose 2
            1 << 3 | 1 << 0,                   // 4 choose 2
            1 << 3 | 1 << 1,                   // 4 choose 2
            1 << 3 | 1 << 2,                   // 4 choose 2
            1 << 2 | 1 << 1 | 1 << 0,          // 4 choose 3
            1 << 3 | 1 << 1 | 1 << 0,          // 4 choose 3
            1 << 3 | 1 << 2 | 1 << 0,          // 4 choose 3
            1 << 3 | 1 << 2 | 1 << 1,          // 4 choose 3
            1 << 3 | 1 << 2 | 1 << 1 | 1 << 0, // 4 choose 4
        ];

        for (idx, item) in items.iter().enumerate() {
            let encoded = encoder.encode(*item);
            assert_eq!(
                encoded, idx as u32,
                "encode({item:0b}) = {encoded} != {idx}"
            );
            let decoded = encoder.decode(encoded);
            assert_eq!(decoded, *item, "decode({encoded}) = {decoded} != {item}");
        }
    }

    #[test]
    fn test_roundtrip_4_2() {
        let encoder = CombinatorialEncoder::<4, 2>::new();
        let items = [
            0,               // 4 choose 0
            1 << 0,          // 4 choose 1
            1 << 1,          // 4 choose 1
            1 << 2,          // 4 choose 1
            1 << 3,          // 4 choose 1
            1 << 1 | 1 << 0, // 4 choose 2
            1 << 2 | 1 << 0, // 4 choose 2
            1 << 2 | 1 << 1, // 4 choose 2
            1 << 3 | 1 << 0, // 4 choose 2
            1 << 3 | 1 << 1, // 4 choose 2
            1 << 3 | 1 << 2, // 4 choose 2
        ];

        for (idx, item) in items.iter().copied().enumerate() {
            let encoded = encoder.encode(item);
            assert_eq!(
                encoded, idx as u32,
                "encode({item:0b}) = {encoded} != {idx}"
            );
            let decoded = encoder.decode(encoded);
            assert_eq!(decoded, item, "decode({encoded}) = {decoded} != {item}");
        }
    }

    #[test]
    fn test_expected_size_34_8() {
        let encoder = CombinatorialEncoder::<34, 8>::new();
        assert_eq!(
            encoder.size_offsets,
            [
                0,
                1,
                1 + 34,
                1 + 34 + 561,
                1 + 34 + 561 + 5984,
                1 + 34 + 561 + 5984 + 46376,
                1 + 34 + 561 + 5984 + 46376 + 278256,
                1 + 34 + 561 + 5984 + 46376 + 278256 + 1344904,
                1 + 34 + 561 + 5984 + 46376 + 278256 + 1344904 + 5379616,
                1 + 34 + 561 + 5984 + 46376 + 278256 + 1344904 + 5379616 + 18156204
            ]
        );

        assert_eq!(
            encoder.maximum_index(),
            1 + 34 + 561 + 5984 + 46376 + 278256 + 1344904 + 5379616 + 18156204
        )
    }
}
