use serde::de::{SeqAccess, Visitor};
use serde::ser::SerializeSeq;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::fmt::{Debug, Formatter};
use std::marker::PhantomData;

// Trait for types that can be packed
pub trait Packable: Copy + Sized + From<u8> + Into<u8> {
    /// Maximum value this type can take + 1 (e.g., 16 for a 4-bit value)
    fn max_value() -> u8;
}

// Implement custom serialization
impl<T: Packable + Serialize, const BITS_PER_ENTRY: usize> Serialize
    for PackedValues<T, BITS_PER_ENTRY>
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // Serialize as a sequence of T
        let mut seq = serializer.serialize_seq(Some(self.count))?;
        for i in 0..self.count {
            // Get each value and serialize it
            if let Some(value) = self.get(i) {
                seq.serialize_element(&value)?;
            }
        }
        seq.end()
    }
}

// Visitor for deserialization
struct PackedValuesVisitor<T: Packable, const BITS_PER_ENTRY: usize> {
    marker: PhantomData<T>,
}

impl<'de, T: Packable + Deserialize<'de>, const BITS_PER_ENTRY: usize> Visitor<'de>
    for PackedValuesVisitor<T, BITS_PER_ENTRY>
{
    type Value = PackedValues<T, BITS_PER_ENTRY>;

    fn expecting(&self, formatter: &mut Formatter) -> fmt::Result {
        formatter.write_str("a sequence of values")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut values = PackedValues::<T, BITS_PER_ENTRY>::new();

        // Deserialize each element and add it to our container
        while let Some(value) = seq.next_element()? {
            values.push(value).map_err(serde::de::Error::custom)?;
        }

        Ok(values)
    }
}

// Implement custom deserialization
impl<'de, T: Packable + Deserialize<'de>, const BITS_PER_ENTRY: usize> Deserialize<'de>
    for PackedValues<T, BITS_PER_ENTRY>
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_seq(PackedValuesVisitor {
            marker: PhantomData,
        })
    }
}

// Generic structure to store packed values in an u128
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct PackedValues<T: Packable, const BITS_PER_ENTRY: usize> {
    data: u128,
    count: usize,
    _marker: PhantomData<T>,
}

impl<T: Packable + Debug, const N: usize> Debug for PackedValues<T, N> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}

impl<T: Packable, const BITS_PER_ENTRY: usize> PackedValues<T, BITS_PER_ENTRY> {
    // Create a new empty packed container
    pub fn new() -> Self {
        // Validate that BITS_PER_ENTRY is valid
        assert!(BITS_PER_ENTRY > 0);

        // Ensure the type can fit in the specified number of bits
        let max_storable_value = (1 << BITS_PER_ENTRY) - 1;
        assert!(
            T::max_value() as usize <= max_storable_value + 1,
            "Type requires more than {} bits to store",
            BITS_PER_ENTRY
        );

        Self {
            data: 0,
            count: 0,
            _marker: PhantomData,
        }
    }

    // Maximum number of entries that can be stored
    pub const MAX_ENTRIES: usize = 128 / BITS_PER_ENTRY;

    // Mask for extracting bits for a single entry
    const ENTRY_MASK: u128 = (1 << BITS_PER_ENTRY) - 1;

    // Get the data value
    pub fn bits(&self) -> u128 {
        self.data
    }

    // Push a new value onto the end if there's space
    pub fn push(&mut self, value: T) -> Result<(), &'static str> {
        if self.count >= Self::MAX_ENTRIES {
            return Err("Cannot store more entries");
        }

        // Each value uses BITS_PER_ENTRY bits, so shift by that * position
        let position = self.count * BITS_PER_ENTRY;

        // Clear any existing bits at the position and set the new bits
        self.data &= !(Self::ENTRY_MASK << position);
        self.data |= (Into::<u8>::into(value) as u128) << position;

        self.count += 1;
        Ok(())
    }

    // Get a value at a specific index
    pub fn get(&self, index: usize) -> Option<T> {
        if index >= self.count {
            return None;
        }

        let position = index * BITS_PER_ENTRY;
        let value = ((self.data >> position) & Self::ENTRY_MASK) as u8;

        Some(T::from(value))
    }

    // Update a value at a specific index
    pub fn set(&mut self, index: usize, value: T) -> Result<(), &'static str> {
        if index >= self.count {
            return Err("Index out of bounds");
        }

        let position = index * BITS_PER_ENTRY;

        // Clear the old value and set the new one
        self.data &= !(Self::ENTRY_MASK << position);
        self.data |= (value.into() as u128) << position;

        Ok(())
    }

    // Number of values currently stored
    pub fn len(&self) -> usize {
        self.count
    }

    // Check if the container is empty
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    // Clear all values
    pub fn clear(&mut self) {
        self.data = 0;
        self.count = 0;
    }

    // Pop the last value
    pub fn pop(&mut self) -> Option<T> {
        if self.count == 0 {
            return None;
        }

        self.count -= 1;
        let position = self.count * BITS_PER_ENTRY;
        let value = ((self.data >> position) & Self::ENTRY_MASK) as u8;

        Some(T::from(value))
    }

    // Return an iterator over all stored values
    pub fn iter(&self) -> PackedIterator<T, BITS_PER_ENTRY> {
        PackedIterator {
            packed: *self,
            current: 0,
        }
    }
}

impl<T: Packable, const BITS_PER_ENTRY: usize> From<u128> for PackedValues<T, BITS_PER_ENTRY> {
    fn from(value: u128) -> Self {
        let data = value;
        let mut count = 0;
        let mut value = value;
        while value & Self::ENTRY_MASK != 0 {
            count += 1;
            value >>= BITS_PER_ENTRY;
        }
        Self {
            data,
            count,
            _marker: PhantomData,
        }
    }
}

// Iterator for the packed values
pub struct PackedIterator<T: Packable, const BITS_PER_ENTRY: usize> {
    packed: PackedValues<T, BITS_PER_ENTRY>,
    current: usize,
}

impl<T: Packable, const BITS_PER_ENTRY: usize> Iterator for PackedIterator<T, BITS_PER_ENTRY> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        if self.current >= self.packed.count {
            return None;
        }

        let result = self.packed.get(self.current);
        self.current += 1;
        result
    }
}
