use savefile_derive::Savefile;
use serde::{Deserialize, Serialize};

#[derive(Savefile, Serialize, Deserialize)]
pub struct FlatStorage<T>
where
    T: 'static,
{
    paths: Vec<T>,
    offsets: Vec<usize>,
}

impl<T: 'static> From<Vec<Vec<T>>> for FlatStorage<T> {
    fn from(ragged: Vec<Vec<T>>) -> Self {
        let num_elem = ragged.len();
        let num_paths = ragged.iter().map(|p| p.len()).sum();

        let mut paths = Vec::with_capacity(num_paths);
        let mut offsets = vec![0; num_elem + 1];

        for (idx, path) in ragged.into_iter().enumerate() {
            offsets[idx + 1] = offsets[idx] + path.len();
            paths.extend(path)
        }

        Self { paths, offsets }
    }
}

impl<T: 'static> FlatStorage<T> {
    pub fn get(&self, idx: usize) -> &[T] {
        let offset = self.offsets[idx];
        let length = self.offsets[idx + 1] - offset;
        &self.paths[offset..offset + length]
    }
}
