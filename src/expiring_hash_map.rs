use futures::ready;
use std::borrow::Borrow;
use std::collections::HashMap;
use std::hash::Hash;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};
use tokio02::time::{delay_queue, DelayQueue, Error};

pub type ExpiredBuffer<K, V> = Vec<(V, delay_queue::Expired<K>)>;

pub struct ExpiringHashMap<K, V> {
    map: HashMap<K, (V, delay_queue::Key)>,
    expiration_queue: DelayQueue<K>,
}

impl<K, V> ExpiringHashMap<K, V>
where
    K: Eq + Hash + Clone,
{
    pub fn insert(&mut self, key: K, value_with_ttl: (V, Duration)) {
        let (value, ttl) = value_with_ttl;
        let delay = self.expiration_queue.insert(key.clone(), ttl);
        self.map.insert(key, (value, delay));
    }

    pub fn insert_at(&mut self, key: K, value_with_deadline: (V, Instant)) {
        let (value, deadline) = value_with_deadline;
        let delay = self
            .expiration_queue
            .insert_at(key.clone(), deadline.into());
        self.map.insert(key, (value, delay));
    }

    pub fn get<Q: ?Sized>(&self, k: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
        Q: Hash + Eq,
    {
        self.map.get(k).map(|&(ref v, _)| v)
    }

    pub fn get_mut<Q: ?Sized>(&mut self, k: &Q) -> Option<&mut V>
    where
        K: Borrow<Q>,
        Q: Hash + Eq,
    {
        self.map.get_mut(k).map(|&mut (ref mut v, _)| v)
    }

    pub fn reset_at<Q: ?Sized>(&mut self, k: &Q, when: Instant) -> Result<(), ()>
    where
        K: Borrow<Q>,
        Q: Hash + Eq,
    {
        let delay_queue_key = self.map.get(k).map(|&(_, ref v)| v).ok_or(())?;
        self.expiration_queue.reset_at(delay_queue_key, when.into());
        Ok(())
    }

    pub fn remove<Q: ?Sized>(&mut self, k: &Q) -> Option<(V, delay_queue::Expired<K>)>
    where
        K: Borrow<Q>,
        Q: Hash + Eq,
    {
        let (value, expiration_queue_key) = self.map.remove(k)?;
        let expired = self.expiration_queue.remove(&expiration_queue_key);
        Some((value, expired))
    }

    pub fn poll_expired(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<Result<ExpiredBuffer<K, V>, (Error, ExpiredBuffer<K, V>)>> {
        let first_key = ready!(self.expiration_queue.poll_expired(cx));
        let first_key = match first_key {
            None => return Poll::Ready(Ok(Vec::new())),
            Some(Err(err)) => return Poll::Ready(Err((err, Vec::new()))),
            Some(Ok(key)) => key,
        };

        let (first_value, _) = self.map.remove(first_key.get_ref()).unwrap();
        let mut buffered = vec![(first_value, first_key)];

        while let Poll::Ready(Some(key)) = self.expiration_queue.poll_expired(cx) {
            let key = match key {
                Ok(key) => key,
                Err(err) => {
                    return Poll::Ready(Err((err, buffered)));
                }
            };

            let (value, _) = self.map.remove(key.get_ref()).unwrap();
            buffered.push((value, key));
        }

        Poll::Ready(Ok(buffered))
    }
}
