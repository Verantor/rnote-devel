use crate::{RnAppWindow, RnCanvas};
use adw::prelude::*;
use gtk4::{CompositeTemplate, Widget, glib, subclass::prelude::*};
use rnote_engine::WidgetFlags;

mod imp {
    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/com/github/flxzt/rnote/ui/searchpanel.ui")]
    pub(crate) struct RnSearchPanel {
        #[template_child]
        pub(crate) search_bar: TemplateChild<gtk4::SearchBar>,
        #[template_child]
        pub(crate) search_entry: TemplateChild<gtk4::SearchEntry>,
        #[template_child]
        pub(crate) content_stack: TemplateChild<gtk4::Stack>,
        #[template_child]
        pub(crate) status_page: TemplateChild<adw::StatusPage>,
        #[template_child]
        pub(crate) results_list: TemplateChild<gtk4::ListBox>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for RnSearchPanel {
        const NAME: &'static str = "RnSearchPanel";
        type Type = super::RnSearchPanel;
        type ParentType = Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for RnSearchPanel {
        fn constructed(&self) {
            self.parent_constructed();
        }

        fn dispose(&self) {
            self.dispose_template();
            while let Some(child) = self.obj().first_child() {
                child.unparent();
            }
        }
    }
    impl WidgetImpl for RnSearchPanel {}
}

glib::wrapper! {
    pub(crate) struct RnSearchPanel(ObjectSubclass<imp::RnSearchPanel>)
        @extends Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for RnSearchPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl RnSearchPanel {
    pub(crate) fn new() -> Self {
        glib::Object::new()
    }

    pub(crate) fn init(&self, window: &RnAppWindow) {
        let imp = self.imp();

        // 1. Handle live search text changes
        imp.search_entry.connect_search_changed(glib::clone!(
            #[weak(rename_to = panel)]
            self,
            #[weak]
            window,
            move |entry| {
                let query = entry.text();
                panel.perform_search(&query, &window);
            }
        ));

        // 2. Handle 'Enter' key to cycle through results on the canvas
        imp.search_entry.connect_activate(glib::clone!(
            #[weak]
            window,
            move |_| {
                if let Some(canvas) = window.active_tab_canvas() {
                    let mut engine_mut = canvas.engine_mut();
                    let mut flags = engine_mut.focus_next_search_result();
                    // Force redraw and view layout update
                    flags.redraw = true;
                    flags.view_modified = true;
                    window.handle_widget_flags(flags, &canvas);
                }
            }
        ));

        // 3. Handle clicking a specific result in the ListBox
        imp.results_list.connect_row_activated(glib::clone!(
            #[weak]
            window,
            move |_, row| {
                let index = row.index() as usize;

                if let Some(canvas) = window.active_tab_canvas() {
                    let mut engine_mut = canvas.engine_mut();
                    let mut flags = engine_mut.focus_search_result_at_index(index);

                    // Force redraw to guarantee the highlight is shown visually
                    flags.redraw = true;
                    flags.view_modified = true;

                    window.handle_widget_flags(flags, &canvas);
                }
            }
        ));

        // 4. Handle "Escape" key behavior
        imp.search_entry.connect_stop_search(glib::clone!(
            #[weak]
            window,
            move |_| {
                // Clicking Esc emits 'stop-search'. We untoggle the header button
                // which handles smoothly returning to the previous sidebar state.
                window.main_header().search_toggle().set_active(false);
            }
        ));

        // Initial setup run
        self.perform_search("", window);
    }

    fn perform_search(&self, query: &str, window: &RnAppWindow) {
        let imp = self.imp();

        // Clear existing list box results
        while let Some(child) = imp.results_list.first_child() {
            imp.results_list.remove(&child);
        }

        let query_trimmed = query.trim();

        // If there's no search query, show the empty state status page
        if query_trimmed.is_empty() {
            imp.status_page
                .set_icon_name(Some("system-search-symbolic"));
            imp.status_page.set_title("Search");
            imp.status_page
                .set_description(Some("Start typing to search the document."));
            imp.content_stack.set_visible_child_name("status");
        }

        if let Some(canvas) = window.active_tab_canvas() {
            let mut engine_mut = canvas.engine_mut();

            // Run the search in the engine store
            let results = engine_mut.search_document(query);

            if !query_trimmed.is_empty() {
                if results.is_empty() {
                    // Show "No Results" state
                    imp.status_page.set_icon_name(Some("edit-find-symbolic"));
                    imp.status_page.set_title("No Results");
                    imp.status_page
                        .set_description(Some("No matching text found."));
                    imp.content_stack.set_visible_child_name("status");
                } else {
                    // Show results list
                    imp.content_stack.set_visible_child_name("results");
                    for _result in results.iter() {
                        let snippet = _result.text.to_string();

                        let row = adw::ActionRow::builder()
                            .title(&snippet)
                            .selectable(true)
                            .activatable(true) // Ensure clicks fire the row_activated signal
                            .build();

                        imp.results_list.append(&row);
                    }
                }
            }

            engine_mut.set_search_results(results);

            let mut flags = WidgetFlags::default();
            flags.redraw = true;
            window.handle_widget_flags(flags, &canvas);
        }
    }

    // Safely opens the search and ensures the SearchBar hasn't disabled itself
    pub(crate) fn open_search(&self) {
        let imp = self.imp();
        imp.search_bar.set_search_mode(true);
        imp.search_entry.grab_focus();
    }
}
