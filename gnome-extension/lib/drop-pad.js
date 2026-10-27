// Real GTK4 Wayland drop surface. GNOME Shell positions this small owned surface
// inside the expanded companion; file paths are sent to the session workspace.
import Gtk from 'gi://Gtk?version=4.0';
import Gdk from 'gi://Gdk?version=4.0';
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
const app = new Gtk.Application({application_id:'ro.novabytecode.Mira.DropPad', flags:Gio.ApplicationFlags.NON_UNIQUE});
let choosing=false;
app.connect('activate',()=>{
    const window=new Gtk.ApplicationWindow({application:app,title:'Mira · Raft temporar',decorated:false,default_width:640,default_height:420});
    const css=new Gtk.CssProvider(); css.load_from_string('window {background:#131e29;color:#eef6f4;border-radius:22px;} button {border-radius:12px;background:#233745;color:#b1f3d0;padding:10px;} .title {font-size:19px;font-weight:bold;} label {color:#c4d6df;}');
    Gtk.StyleContext.add_provider_for_display(Gdk.Display.get_default(),css,Gtk.STYLE_PROVIDER_PRIORITY_APPLICATION);
    const box=new Gtk.Box({orientation:Gtk.Orientation.VERTICAL,spacing:14,margin_top:22,margin_bottom:22,margin_start:24,margin_end:24}); window.set_child(box);
    box.append(new Gtk.Label({label:'◖  ◗   mira',margin_top:20,margin_bottom:25}));
    const title=new Gtk.Label({label:'⇩ Lasă fișierele aici'});title.add_css_class('title');box.append(title);
    const feedback=new Gtk.Label({label:'Shortcuturi temporare. Originalele rămân la locul lor.',wrap:true});box.append(feedback);
    const add=files=>{
        const paths=files.map(f=>f.get_path()).filter(Boolean); if (!paths.length) return;
        feedback.label='Adaug în raft…';
        Gio.DBus.session.call('ro.novabytecode.Mira','/ro/novabytecode/Mira','ro.novabytecode.Mira.Workspace','AddPaths',new GLib.Variant('(as)',[paths]),null,Gio.DBusCallFlags.NONE,15000,null,(conn,res)=>{
            try {conn.call_finish(res);app.quit();}catch(e){feedback.label=e.message;}
        });
    };
    const target=Gtk.DropTarget.new(Gdk.FileList.$gtype,Gdk.DragAction.COPY);
    target.connect('drop',(_target,list)=>{add(list.get_files());return true;});box.add_controller(target);
    const actions=new Gtk.Box({spacing:10,halign:Gtk.Align.CENTER});box.append(actions);
    for (const [label,folder] of [['Fișiere',false],['Folder',true]]) {
        const button=new Gtk.Button({label});actions.append(button);
        button.connect('clicked',()=>{
            choosing=true;
            const dialog=new Gtk.FileChooserNative({title:folder?'Alege folderul':'Alege fișierele',transient_for:window,action:folder?Gtk.FileChooserAction.SELECT_FOLDER:Gtk.FileChooserAction.OPEN,select_multiple:!folder,accept_label:'Adaugă',cancel_label:'Renunță'});
            dialog.connect('response',(_d,response)=>{choosing=false;if(response===Gtk.ResponseType.ACCEPT){const model=dialog.get_files();add(Array.from({length:model.get_n_items()},(_,i)=>model.get_item(i)));}dialog.destroy();});dialog.show();
        });
    }
    const close=new Gtk.Button({label:'Închide'});actions.append(close);close.connect('clicked',()=>app.quit());
    window.present();
    GLib.timeout_add(GLib.PRIORITY_DEFAULT,30000,()=>{if(choosing)return GLib.SOURCE_CONTINUE;app.quit();return GLib.SOURCE_REMOVE;});
});
app.run([]);
