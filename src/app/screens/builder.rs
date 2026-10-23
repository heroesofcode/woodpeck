use super::super::{Woodpeck, style, widgets};
use crate::{
    service::Job,
    sets::{CYCLE_CARDS, PRESETS, PresetKind, THEME_FOCUS, difficulty_caption},
};
use eframe::egui::{self, RichText, Ui};
impl Woodpeck {
    pub(in crate::app) fn builder_screen(&mut self, ui: &mut Ui) {
        widgets::title(
            ui,
            "MAKE IT YOURS",
            "Build a training set",
            "Choose your focus. Keep the positions. Watch the patterns stick.",
        );
        if ui.available_width() > 800.0 {
            ui.horizontal_top(|ui| {
                ui.allocate_ui_with_layout(
                    egui::vec2(170.0, ui.available_height()),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        for (i, preset) in PRESETS.into_iter().enumerate() {
                            if i == 1 {
                                ui.add_space(15.0);
                                widgets::caption(ui, "WEAKNESS");
                            }
                            if matches!(preset.kind, PresetKind::Level(800, 1000)) {
                                ui.add_space(15.0);
                                widgets::caption(ui, "LEVEL");
                            }
                            let selected = self.builder.preset_id.as_deref() == Some(preset.id);
                            if ui
                                .add_sized(
                                    [160.0, 36.0],
                                    egui::Button::new(preset.label).selected(selected),
                                )
                                .on_hover_text(preset.description)
                                .clicked()
                            {
                                self.builder = preset.builder(self.settings.user_rating);
                                self.builder.shuffle_each_cycle = self.settings.shuffle_each_cycle;
                                self.creation = None;
                            }
                        }
                    },
                );
                ui.add_space(18.0);
                ui.vertical(|ui| {
                    self.builder_form(ui);
                });
            });
        } else {
            egui::ComboBox::from_id_salt("preset")
                .width(ui.available_width() - 12.0)
                .selected_text(
                    self.builder
                        .preset_id
                        .as_deref()
                        .and_then(crate::sets::Preset::find)
                        .map_or("Custom set", |p| p.label),
                )
                .show_ui(ui, |ui| {
                    for p in PRESETS {
                        if ui
                            .selectable_label(
                                self.builder.preset_id.as_deref() == Some(p.id),
                                p.label,
                            )
                            .clicked()
                        {
                            self.builder = p.builder(self.settings.user_rating);
                            self.builder.shuffle_each_cycle = self.settings.shuffle_each_cycle;
                            self.creation = None;
                        }
                    }
                });
            ui.add_space(12.0);
            self.builder_form(ui);
        }
    }
    fn builder_form(&mut self, ui: &mut Ui) {
        widgets::card(ui).show(ui,|ui| {
            ui.set_width((ui.available_width()-1.0).max(0.0));
            ui.heading("Basic info");ui.label("Name your set and choose an optional tactical focus.");ui.add_space(8.0);
            widgets::caption(ui,"SET NAME");ui.add(egui::TextEdit::singleline(&mut self.builder.name).desired_width(ui.available_width()));
            widgets::caption(ui,"THEME FOCUS");
            egui::ComboBox::from_id_salt("focus").width((ui.available_width()-12.0).min(400.0)).selected_text(self.builder.focus_theme.as_deref().map_or_else(||"Any theme".into(),crate::sets::theme_label)).show_ui(ui,|ui|{for (key,label) in THEME_FOCUS{ui.selectable_value(&mut self.builder.focus_theme,(!key.is_empty()).then(||key.into()),label);}});
            ui.add_space(14.0);ui.separator();ui.add_space(12.0);
            ui.heading("Difficulty");ui.label(format!("A little below your peak works best. Your puzzle rating is {}.",self.settings.user_rating));ui.add_space(8.0);
            widgets::caption(ui,"TARGET RATING");let (low,high)=self.builder.target_limits(self.settings.user_rating);
            self.builder.target_rating=self.builder.target_rating.clamp(low,high);
            ui.spacing_mut().slider_width=(ui.available_width()-74.0).clamp(100.0,500.0);
            ui.add(egui::Slider::new(&mut self.builder.target_rating,low..=high));
            ui.horizontal_wrapped(|ui|{widgets::chip(ui,difficulty_caption(self.builder.target_rating,self.settings.user_rating),style::color(self.palette.semantic().accent));ui.small(format!("{low}–{high}"));});
            ui.checkbox(&mut self.builder.restrict_recommended,"Restrict to recommended range");
            ui.add_space(6.0);widgets::caption(ui,format!("RATING VARIANCE · {}–{}",self.builder.band().0,self.builder.band().1));
            ui.add(egui::Slider::new(&mut self.builder.variance,50..=200).step_by(25.0).prefix("±"));ui.small("Narrow (±50)                              Wide (±200)");
            ui.add_space(14.0);ui.separator();ui.add_space(12.0);
            ui.heading("Training volume");ui.label("Start with 50–100 puzzles if you're unsure.");
            widgets::caption(ui,"TOTAL PUZZLES");ui.add(egui::Slider::new(&mut self.builder.size,20..=500));ui.add_space(6.0);widgets::caption(ui,"TRAINING CYCLES");
            let width=((ui.available_width()-20.0)/3.0).max(50.0);
            ui.horizontal(|ui|{for (cycles,label) in CYCLE_CARDS {let selected=self.builder.target_cycles==cycles;let response=ui.add_sized([width,76.0],egui::Button::new(RichText::new(format!("{cycles} cycles\n{label}")).size(if width<100.0{11.0}else{14.0})).wrap().selected(selected).corner_radius(10));if response.clicked(){self.builder.target_cycles=cycles;}}});
            ui.horizontal(|ui|{ui.label("Custom cycles");ui.add(egui::DragValue::new(&mut self.builder.target_cycles).range(1..=10));});ui.checkbox(&mut self.builder.shuffle_each_cycle,"Shuffle each cycle");
            ui.add_space(8.0);ui.small("Positions stay fixed across cycles. Puzzle ratings are separate from playing ratings.");ui.add_space(12.0);
            let (minimum,maximum)=self.builder.estimated_time();
            widgets::caption(ui,"EST. COMPLETION TIME");ui.strong(format!("{:.1}–{:.1} hours",minimum.as_secs_f64()/3600.0,maximum.as_secs_f64()/3600.0));
            if widgets::primary(ui,"Create Set  >").clicked(){match self.service.create_set(&self.builder,chrono::Local::now().timestamp().unsigned_abs(),chrono::Local::now().timestamp()){Ok(c)=>self.created(c),Err(e)=>self.error(e)}}
            if let Some(c)=&self.creation && c.set_id.is_none(){ui.add_space(10.0);ui.colored_label(style::color(self.palette.semantic().warning),format!("{} puzzles short. Found {} of {} after widening the band from {}–{} to {}–{}.",c.shortfall(),c.available,c.requested,c.original_band.0,c.original_band.1,c.used_band.0,c.used_band.1));
                if ui.add_enabled(!self.service.is_busy(),egui::Button::new("Fetch more from Lichess")).clicked(){self.submit(Job::CreateSetWithFallback {params:self.builder.clone(),seed:chrono::Local::now().timestamp().unsigned_abs(),now:chrono::Local::now().timestamp()});}
            }
            if self.service.is_busy(){ui.horizontal_wrapped(|ui|{ui.spinner();ui.label(&self.status);});ui.add(egui::ProgressBar::new(0.0).animate(true).text("Background import / fetch"));}
        });
    }
}
