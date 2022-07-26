// Copyright (c) Facebook, Inc. and its affiliates.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::cell::Ref;
use std::cell::RefCell;
use std::cell::RefMut;
use std::collections::HashMap;
use std::rc::Rc;

use cursive::utils::markup::StyledString;
use cursive::view::Nameable;
use cursive::views::NamedView;
use cursive::views::SelectView;
use cursive::views::ViewRef;
use cursive::Cursive;

use model::ProcessCpuModelFieldId;
use model::ProcessIoModelFieldId;
use model::ProcessMemoryModelFieldId;
use model::ProcessModel;
use model::SingleProcessModelFieldId;

use crate::process_tabs::default_tabs::PROCESS_CPU_TAB;
use crate::process_tabs::default_tabs::PROCESS_GENERAL_TAB;
use crate::process_tabs::default_tabs::PROCESS_IO_TAB;
use crate::process_tabs::default_tabs::PROCESS_MEM_TAB;
use crate::process_tabs::ProcessTab;
use crate::stats_view::ColumnTitles;
use crate::stats_view::StateCommon;
use crate::stats_view::StatsView;
use crate::stats_view::ViewBridge;
use crate::ViewState;

pub type ViewType = StatsView<ProcessView>;

#[derive(Default)]
pub struct ProcessState {
    pub filter: Option<String>,
    pub cgroup_filter: Option<String>,
    pub pids_filter: Option<Vec<i32>>,
    // For zoomed view, we should save current filter to here and reset the
    // filter when go back to cgroup or process view.
    pub filter_cache_for_zoom: Option<String>,
    pub current_selected_pid: Option<i32>,
    pub sort_order: Option<SingleProcessModelFieldId>,
    pub sort_tags: HashMap<String, &'static ProcessTab>,
    pub reverse: bool,
    pub fold: bool,
    pub model: Rc<RefCell<ProcessModel>>,
}

impl StateCommon for ProcessState {
    type ModelType = ProcessModel;
    type TagType = SingleProcessModelFieldId;
    type KeyType = i32;

    fn get_filter(&mut self) -> &mut Option<String> {
        &mut self.filter
    }

    fn set_sort_tag(&mut self, sort_order: Self::TagType, reverse: &mut bool) -> bool {
        let sort_order = Some(sort_order);
        if self.sort_order == sort_order {
            *reverse = !*reverse;
        } else {
            *reverse = true;
            self.sort_order = sort_order;
        }
        self.reverse = *reverse;
        true
    }

    fn set_sort_tag_from_tab_idx(&mut self, tab: &str, idx: usize, reverse: &mut bool) -> bool {
        let sort_order = match idx {
            0 => Self::TagType::Comm,
            1 => Self::TagType::Cgroup,
            _ => self
                .sort_tags
                .get(tab)
                .unwrap_or_else(|| panic!("Fail to find tab: {}", tab))
                .view_items
                .get(idx - 2)
                .expect("Out of title scope")
                .field_id
                .to_owned(),
        };

        self.set_sort_tag(sort_order, reverse)
    }

    fn set_sort_string(&mut self, selection: &str, reverse: &mut bool) -> bool {
        use std::str::FromStr;
        match Self::TagType::from_str(selection) {
            Ok(field_id) => self.set_sort_tag(field_id, reverse),
            Err(_) => false,
        }
    }

    fn get_model(&self) -> Ref<Self::ModelType> {
        self.model.borrow()
    }

    fn get_model_mut(&self) -> RefMut<Self::ModelType> {
        self.model.borrow_mut()
    }

    fn new(model: Rc<RefCell<Self::ModelType>>) -> Self {
        let mut sort_tags = HashMap::new();
        sort_tags.insert("General".into(), &*PROCESS_GENERAL_TAB);
        sort_tags.insert("CPU".into(), &*PROCESS_CPU_TAB);
        sort_tags.insert("Mem".into(), &*PROCESS_MEM_TAB);
        sort_tags.insert("I/O".into(), &*PROCESS_IO_TAB);
        Self {
            filter: None,
            cgroup_filter: None,
            pids_filter: None,
            filter_cache_for_zoom: None,
            current_selected_pid: None,
            sort_order: None,
            sort_tags,
            reverse: false,
            fold: false,
            model,
        }
    }
}

impl ProcessState {
    fn set_sort_order(&mut self, tag: SingleProcessModelFieldId) {
        self.sort_order = Some(tag);
    }

    fn set_reverse(&mut self, reverse: bool) {
        self.reverse = reverse;
    }

    pub fn toggle_fold(&mut self) {
        self.fold = !self.fold;
    }

    pub fn handle_state_for_entering_zoom(&mut self, current_selection: String) {
        self.cgroup_filter = Some(current_selection);
        std::mem::swap(&mut self.filter_cache_for_zoom, &mut self.filter);
        self.filter = None;
        self.pids_filter = None;
    }

    pub fn reset_state_for_quiting_zoom(&mut self) {
        std::mem::swap(&mut self.filter, &mut self.filter_cache_for_zoom);
        self.cgroup_filter = None;
        self.filter_cache_for_zoom = None;
        self.pids_filter = None;
    }

    pub fn handle_state_for_entering_pids_zoom(&mut self, current_selection: Vec<i32>) {
        self.pids_filter = Some(current_selection);
        std::mem::swap(&mut self.filter, &mut self.filter_cache_for_zoom);
        self.filter = None;
        self.cgroup_filter = None;
    }

    pub fn get_cgroup_for_selected_pid(&self) -> Option<String> {
        self.get_model()
            .processes
            .get(&self.current_selected_pid?)
            .and_then(|spm| spm.cgroup.clone())
    }
}

pub struct ProcessView {
    tab: &'static ProcessTab,
}

impl ProcessView {
    pub fn new(c: &mut Cursive) -> NamedView<ViewType> {
        let list = SelectView::<i32>::new();
        let tabs = vec!["General".into(), "CPU".into(), "Mem".into(), "I/O".into()];
        let mut tabs_map: HashMap<String, ProcessView> = HashMap::new();
        tabs_map.insert(
            "General".into(),
            Self {
                tab: &*PROCESS_GENERAL_TAB,
            },
        );
        tabs_map.insert(
            "CPU".into(),
            Self {
                tab: &*PROCESS_CPU_TAB,
            },
        );
        tabs_map.insert(
            "Mem".into(),
            Self {
                tab: &*PROCESS_MEM_TAB,
            },
        );
        tabs_map.insert(
            "I/O".into(),
            Self {
                tab: &*PROCESS_IO_TAB,
            },
        );
        let user_data = c
            .user_data::<ViewState>()
            .expect("No data stored in Cursive Object!");
        StatsView::new(
            "process",
            tabs,
            tabs_map,
            list,
            ProcessState::new(user_data.process.clone()),
            user_data.event_controllers.clone(),
            user_data.cmd_controllers.clone(),
        )
        .feed_data(c)
        .on_event('P', |c| {
            let mut view = Self::get_process_view(c);
            view.state
                .borrow_mut()
                .set_sort_order(SingleProcessModelFieldId::Pid);
            view.state.borrow_mut().set_reverse(false);
            view.refresh(c)
        })
        .on_event('C', |c| {
            let mut view = Self::get_process_view(c);
            view.state
                .borrow_mut()
                .set_sort_order(SingleProcessModelFieldId::Cpu(
                    ProcessCpuModelFieldId::UsagePct,
                ));
            view.state.borrow_mut().set_reverse(true);
            view.refresh(c)
        })
        .on_event('N', |c| {
            let mut view = Self::get_process_view(c);
            view.state
                .borrow_mut()
                .set_sort_order(SingleProcessModelFieldId::Comm);
            view.state.borrow_mut().set_reverse(false);
            view.refresh(c)
        })
        .on_event('M', |c| {
            let mut view = Self::get_process_view(c);
            view.state
                .borrow_mut()
                .set_sort_order(SingleProcessModelFieldId::Mem(
                    ProcessMemoryModelFieldId::RssBytes,
                ));
            view.state.borrow_mut().set_reverse(true);
            view.refresh(c)
        })
        .on_event('D', |c| {
            let mut view = Self::get_process_view(c);
            view.state
                .borrow_mut()
                .set_sort_order(SingleProcessModelFieldId::Io(
                    ProcessIoModelFieldId::RwbytesPerSec,
                ));
            view.state.borrow_mut().set_reverse(true);
            view.refresh(c)
        })
        .with_name(Self::get_view_name())
    }

    pub fn get_process_view(c: &mut Cursive) -> ViewRef<ViewType> {
        ViewType::get_view(c)
    }

    pub fn refresh(c: &mut Cursive) {
        let mut view = Self::get_process_view(c);
        view.refresh(c);
    }
}

impl ViewBridge for ProcessView {
    type StateType = ProcessState;
    fn get_view_name() -> &'static str {
        "process_view"
    }
    fn get_titles(&self) -> ColumnTitles {
        self.tab.get_titles()
    }

    fn get_rows(
        &mut self,
        state: &Self::StateType,
        offset: Option<usize>,
    ) -> Vec<(StyledString, i32)> {
        self.tab.get_rows(state, offset)
    }

    fn on_select_update_state(state: &mut Self::StateType, selected_key: Option<&i32>) {
        state.current_selected_pid = selected_key.cloned();
    }

    fn on_select_update_cmd_palette(state: &Self::StateType, selected_key: &i32) -> String {
        state
            .get_model()
            .processes
            .get(selected_key /* pid */)
            .map_or("?".to_string(), |spm| {
                spm.cmdline.clone().unwrap_or_else(|| "?".to_string())
            })
    }
}
