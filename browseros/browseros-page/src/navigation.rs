use browseros_bridge::identifiers::NavigationId;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone)]
pub struct NavigationEntry {
    pub url: String,
    pub title: String,
    pub timestamp: DateTime<Utc>,
    pub navigation_id: NavigationId,
}

impl NavigationEntry {
    pub fn new(url: String, title: String, navigation_id: NavigationId) -> Self {
        NavigationEntry {
            url,
            title,
            timestamp: Utc::now(),
            navigation_id,
        }
    }
}

#[derive(Debug, Clone)]
pub struct NavigationHistory {
    entries: Vec<NavigationEntry>,
    current_index: i64,
    max_entries: usize,
}

impl NavigationHistory {
    pub fn new(max_entries: usize) -> Self {
        let cap = max_entries.min(256);
        NavigationHistory {
            entries: Vec::with_capacity(cap),
            current_index: -1,
            max_entries: cap,
        }
    }

    pub fn push(&mut self, entry: NavigationEntry) {
        if self.current_index >= 0 {
            let keep = (self.current_index + 1) as usize;
            self.entries.truncate(keep);
        }
        if self.entries.len() >= self.max_entries {
            self.entries.remove(0);
        }
        self.entries.push(entry);
        self.current_index = (self.entries.len() - 1) as i64;
    }

    pub fn current(&self) -> Option<&NavigationEntry> {
        if self.current_index < 0 || self.current_index as usize >= self.entries.len() {
            return None;
        }
        Some(&self.entries[self.current_index as usize])
    }

    pub fn current_mut(&mut self) -> Option<&mut NavigationEntry> {
        if self.current_index < 0 || self.current_index as usize >= self.entries.len() {
            return None;
        }
        Some(&mut self.entries[self.current_index as usize])
    }

    pub fn can_go_back(&self) -> bool {
        self.current_index > 0
    }

    pub fn can_go_forward(&self) -> bool {
        self.current_index >= 0 && (self.current_index as usize) < self.entries.len() - 1
    }

    pub fn go_back(&mut self) -> Option<&NavigationEntry> {
        if !self.can_go_back() {
            return None;
        }
        self.current_index -= 1;
        self.current()
    }

    pub fn go_forward(&mut self) -> Option<&NavigationEntry> {
        if !self.can_go_forward() {
            return None;
        }
        self.current_index += 1;
        self.current()
    }

    pub fn entries(&self) -> &[NavigationEntry] {
        &self.entries
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn current_index(&self) -> i64 {
        self.current_index
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.current_index = -1;
    }

    pub fn max_entries(&self) -> usize {
        self.max_entries
    }

    pub fn set_max_entries(&mut self, max: usize) {
        self.max_entries = max;
        if self.entries.len() > max {
            let excess = self.entries.len() - max;
            let new_idx = self.current_index.saturating_sub(excess as i64);
            for _ in 0..excess {
                self.entries.remove(0);
            }
            self.current_index = new_idx;
        }
    }

    pub fn navigate_to_index(&mut self, index: usize) -> Option<&NavigationEntry> {
        if index >= self.entries.len() {
            return None;
        }
        self.current_index = index as i64;
        self.current()
    }
}

impl Default for NavigationHistory {
    fn default() -> Self {
        Self::new(50)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(url: &str, title: &str) -> NavigationEntry {
        NavigationEntry::new(url.into(), title.into(), NavigationId::new())
    }

    #[test]
    fn test_empty_history() {
        let hist = NavigationHistory::new(50);
        assert!(hist.is_empty());
        assert_eq!(hist.len(), 0);
        assert!(hist.current().is_none());
        assert!(!hist.can_go_back());
        assert!(!hist.can_go_forward());
    }

    #[test]
    fn test_push_and_current() {
        let mut hist = NavigationHistory::new(50);
        hist.push(entry("https://example.com", "Example"));
        assert!(!hist.is_empty());
        assert_eq!(hist.len(), 1);
        assert_eq!(hist.current().unwrap().url, "https://example.com");
        assert_eq!(hist.current().unwrap().title, "Example");
    }

    #[test]
    fn test_go_back_and_forward() {
        let mut hist = NavigationHistory::new(50);
        hist.push(entry("https://page1.com", "Page 1"));
        hist.push(entry("https://page2.com", "Page 2"));
        hist.push(entry("https://page3.com", "Page 3"));

        assert_eq!(hist.current().unwrap().url, "https://page3.com");
        assert!(hist.can_go_back());
        assert!(!hist.can_go_forward());

        let back = hist.go_back();
        assert!(back.is_some());
        assert_eq!(back.unwrap().url, "https://page2.com");

        let back = hist.go_back();
        assert_eq!(back.unwrap().url, "https://page1.com");
        assert!(!hist.can_go_back());

        let fwd = hist.go_forward();
        assert_eq!(fwd.unwrap().url, "https://page2.com");
    }

    #[test]
    fn test_push_truncates_forward() {
        let mut hist = NavigationHistory::new(50);
        hist.push(entry("https://page1.com", "Page 1"));
        hist.push(entry("https://page2.com", "Page 2"));
        hist.push(entry("https://page3.com", "Page 3"));

        hist.go_back();
        hist.go_back();
        hist.push(entry("https://new-page.com", "New Page"));

        assert_eq!(hist.current().unwrap().url, "https://new-page.com");
        assert_eq!(hist.len(), 2);
        assert!(hist.can_go_back());
        assert!(!hist.can_go_forward());
    }

    #[test]
    fn test_max_entries_enforced() {
        let mut hist = NavigationHistory::new(3);
        hist.push(entry("https://page1.com", "Page 1"));
        hist.push(entry("https://page2.com", "Page 2"));
        hist.push(entry("https://page3.com", "Page 3"));
        hist.push(entry("https://page4.com", "Page 4"));

        assert_eq!(hist.len(), 3);
        assert_eq!(hist.current().unwrap().url, "https://page4.com");
    }

    #[test]
    fn test_clear() {
        let mut hist = NavigationHistory::new(50);
        hist.push(entry("https://page1.com", "Page 1"));
        hist.push(entry("https://page2.com", "Page 2"));
        hist.clear();
        assert!(hist.is_empty());
        assert!(hist.current().is_none());
    }

    #[test]
    fn test_go_back_empty_returns_none() {
        let mut hist = NavigationHistory::new(50);
        assert!(hist.go_back().is_none());
        assert!(hist.go_forward().is_none());
    }

    #[test]
    fn test_navigate_to_index() {
        let mut hist = NavigationHistory::new(50);
        hist.push(entry("https://page1.com", "Page 1"));
        hist.push(entry("https://page2.com", "Page 2"));
        hist.push(entry("https://page3.com", "Page 3"));

        let entry = hist.navigate_to_index(0);
        assert_eq!(entry.unwrap().url, "https://page1.com");
        assert_eq!(hist.current_index(), 0);

        let entry = hist.navigate_to_index(2);
        assert_eq!(entry.unwrap().url, "https://page3.com");
        assert_eq!(hist.current_index(), 2);
    }

    #[test]
    fn test_navigate_to_invalid_index() {
        let mut hist = NavigationHistory::new(50);
        hist.push(entry("https://page1.com", "Page 1"));
        assert!(hist.navigate_to_index(5).is_none());
    }

    #[test]
    fn test_current_mut() {
        let mut hist = NavigationHistory::new(50);
        hist.push(entry("https://page1.com", "Page 1"));
        if let Some(entry) = hist.current_mut() {
            entry.title = "Updated".into();
        }
        assert_eq!(hist.current().unwrap().title, "Updated");
    }

    #[test]
    fn test_current_mut_empty_returns_none() {
        let mut hist = NavigationHistory::new(50);
        assert!(hist.current_mut().is_none());
    }

    #[test]
    fn test_set_max_entries_truncates() {
        let mut hist = NavigationHistory::new(50);
        hist.push(entry("https://page1.com", "Page 1"));
        hist.push(entry("https://page2.com", "Page 2"));
        hist.push(entry("https://page3.com", "Page 3"));
        hist.set_max_entries(1);
        assert_eq!(hist.len(), 1);
        assert_eq!(hist.current().unwrap().url, "https://page3.com");
    }

    #[test]
    fn test_max_entries_capped() {
        let hist = NavigationHistory::new(500);
        assert_eq!(hist.max_entries(), 256);
    }

    #[test]
    fn test_default_max_entries() {
        let hist = NavigationHistory::default();
        assert_eq!(hist.max_entries(), 50);
    }

    #[test]
    fn test_entries_slice() {
        let mut hist = NavigationHistory::new(50);
        hist.push(entry("https://page1.com", "Page 1"));
        hist.push(entry("https://page2.com", "Page 2"));
        assert_eq!(hist.entries().len(), 2);
    }
}
