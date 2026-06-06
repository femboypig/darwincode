use crate::app::load_custom_agents;

#[derive(Debug, Default)]
pub struct AgentPickerState {
    pub agents: Vec<(Option<String>, String)>, // (agent_id, display_name)
    pub selected: usize,
    pub query: String,
}

impl AgentPickerState {
    pub fn new(current_agent: &Option<String>) -> Self {
        let mut agents = vec![(None, "Standard Agent (None)".to_owned())];

        let custom = load_custom_agents();
        let mut sorted: Vec<_> = custom.into_iter().collect();
        sorted.sort_by(|a, b| a.1.name.cmp(&b.1.name));

        for (id, config) in sorted {
            agents.push((Some(id), config.name));
        }

        let selected = agents
            .iter()
            .position(|(id, _)| id == current_agent)
            .unwrap_or(0);

        Self {
            agents,
            selected,
            query: String::new(),
        }
    }

    pub fn filtered_indices(&self) -> Vec<usize> {
        if self.query.is_empty() {
            return (0..self.agents.len()).collect();
        }
        let q = self.query.to_lowercase();
        self.agents
            .iter()
            .enumerate()
            .filter(|(_, (_, name))| name.to_lowercase().contains(&q))
            .map(|(i, _)| i)
            .collect()
    }

    pub fn selected_agent(&self) -> Option<(Option<String>, String)> {
        let filtered = self.filtered_indices();
        let pos = self.selected.min(filtered.len().saturating_sub(1));
        filtered
            .get(pos)
            .and_then(|&idx| self.agents.get(idx).cloned())
    }

    pub fn select_next(&mut self) {
        let len = self.filtered_indices().len();
        if len > 0 {
            self.selected = (self.selected + 1) % len;
        }
    }

    pub fn select_previous(&mut self) {
        let len = self.filtered_indices().len();
        if len > 0 {
            self.selected = self.selected.checked_sub(1).unwrap_or_else(|| len - 1);
        }
    }

    pub fn push_query(&mut self, c: char) {
        self.query.push(c);
        self.selected = 0;
    }

    pub fn pop_query(&mut self) {
        self.query.pop();
        self.selected = 0;
    }

    pub fn clear_query(&mut self) {
        self.query.clear();
        self.selected = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_picker_state() {
        let mut picker = AgentPickerState::new(&None);
        assert_eq!(picker.selected, 0);
        assert_eq!(picker.selected_agent().unwrap().0, None);

        picker
            .agents
            .push((Some("test-agent".to_owned()), "Test Agent".to_owned()));

        picker.select_next();
        assert_eq!(picker.selected, 1);
        assert_eq!(
            picker.selected_agent().unwrap().0,
            Some("test-agent".to_owned())
        );

        picker.push_query('e');
        let filtered = picker.filtered_indices();
        // Standard Agent (None) and Test Agent both contain 'e' (case-insensitive)
        assert_eq!(filtered.len(), 2);

        picker.pop_query();
        assert_eq!(picker.query, "");

        picker.clear_query();
        picker.select_previous();
    }
}
