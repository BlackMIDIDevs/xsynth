use std::ops::{Deref, DerefMut};

pub struct Cache<T>(Option<T>);

pub struct CacheGuard<'a, T> {
    value: Option<T>,
    cache: &'a mut Cache<T>,
}

impl<T> Cache<T> {
    pub fn new(value: T) -> Cache<T> {
        Cache(Some(value))
    }

    pub fn get<'a>(&'a mut self) -> CacheGuard<'a, T> {
        match self.0.take() {
            None => panic!("Tried to fetch cache twice"),
            Some(v) => CacheGuard {
                value: Some(v),
                cache: self,
            },
        }
    }
}

impl<'a, T> Drop for CacheGuard<'a, T> {
    fn drop(&mut self) {
        self.cache.0.insert(self.value.take().unwrap());
    }
}

impl<'a, T> Deref for CacheGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.value.as_ref().unwrap()
    }
}

impl<'a, T> DerefMut for CacheGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.value.as_mut().unwrap()
    }
}

#[cfg(test)]
mod test {
    use crate::helpers::Cache;

    #[test]
    fn test_cache() {
        let mut cache = Cache::new(vec![1, 2, 3]);
        {
            let mut vec = cache.get();
            assert_eq!(vec[0], 1);
            vec[0] = 5;
            assert_eq!(vec[0], 5);
        }

        {
            let vec = cache.get();
            assert_eq!(vec[0], 5);
        }
    }
}