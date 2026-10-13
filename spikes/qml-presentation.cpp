// SPDX-License-Identifier: GPL-3.0-only
// Measurement-only preload: Qt continues owning its connection and windows.
#include <QGuiApplication>
#include <QQuickWindow>
#include <QTimer>
#include <QQmlEngine>
#include <QJSEngine>
#include <atomic>
#include <qpa/qplatformnativeinterface.h>
#include <wayland-client.h>
#include "presentation-time-client-protocol.h"
#include <algorithm>
#include <cstdio>
#include <map>
#include <string>
#include <cstdlib>
#include <mutex>
#include <vector>
#include <time.h>
struct Samples { std::vector<double> intervals; uint64_t first=0,last=0; uint32_t refresh=0,kind=0; unsigned discarded=0; };
static std::map<wl_surface*, Samples> samples;
static std::atomic<unsigned> generation=0;
static std::map<wl_surface*,unsigned> last_generation;
struct Feedback {wl_surface *surface;unsigned generation;};
class TimingProbe:public QObject {
    Q_OBJECT
public:
    Q_INVOKABLE void commit(unsigned value) {generation.store(value);}
};
static std::mutex lock;
static wp_presentation *presentation=nullptr;
static wl_display *display=nullptr;
static double percentile(std::vector<double> values, double p) {
    if(values.empty()) return -1;
    std::sort(values.begin(),values.end()); return values[size_t((values.size()-1)*p)];
}
static void sync_output(void*, struct wp_presentation_feedback*, wl_output*) {}
static void presented(void *data, struct wp_presentation_feedback *feedback,uint32_t hi,uint32_t lo,uint32_t ns,uint32_t refresh,uint32_t,uint32_t,uint32_t kind) {
    std::lock_guard<std::mutex> guard(lock);
    auto *tag=static_cast<Feedback*>(data);
    auto &s=samples[tag->surface];
    uint64_t time=((uint64_t(hi)<<32)|lo)*1000000000+ns;
    if(!s.first) s.first=time;
    if(s.last && time>s.last && time>s.first+2000000000) s.intervals.push_back(double(time-s.last)/1000000);
    if(!s.last) std::fprintf(stderr,"QML_READY presentation=true\n");
    if(!last_generation.count(tag->surface) || last_generation[tag->surface]!=tag->generation) {
        std::fprintf(stderr,"QML_UI_PRESENTED generation=%u monotonic_ns=%llu\n",tag->generation,static_cast<unsigned long long>(time));
        last_generation[tag->surface]=tag->generation;
    }
    delete tag;
    s.last=time; s.refresh=refresh; s.kind=kind; wp_presentation_feedback_destroy(feedback);
}
static void discarded(void *data,struct wp_presentation_feedback *feedback) {
    std::lock_guard<std::mutex> guard(lock); auto *tag=static_cast<Feedback*>(data);samples[tag->surface].discarded++;delete tag;
    wp_presentation_feedback_destroy(feedback);
}
static const wp_presentation_feedback_listener feedback_listener={sync_output,presented,discarded};
static void clock_id(void*,wp_presentation*,uint32_t clock) { std::fprintf(stderr,"QML_PRESENTATION clock_id=%u\n",clock); }
static const wp_presentation_listener presentation_listener={clock_id};
static void global(void*,wl_registry *registry,uint32_t name,const char *interface,uint32_t version) {
    if(std::string(interface)=="wp_presentation") {
        presentation=static_cast<wp_presentation*>(wl_registry_bind(registry,name,&wp_presentation_interface,std::min(version,2u)));
        wp_presentation_add_listener(presentation,&presentation_listener,nullptr);
    }
}
static void removed(void*,wl_registry*,uint32_t) {}
static const wl_registry_listener registry_listener={global,removed};
static void report() {
    std::lock_guard<std::mutex> guard(lock);
    for(auto &[surface,s]:samples) std::fprintf(stderr,"QML_PRESENTED surface=%p samples=%zu p50_ms=%.3f p99_ms=%.3f refresh_ns=%u kind=%u discarded=%u\n",static_cast<void*>(surface),s.intervals.size(),percentile(s.intervals,.5),percentile(s.intervals,.99),s.refresh,s.kind,s.discarded);
}
static std::vector<QQuickWindow*> tracked;
static void attach(QQuickWindow *quick) {
    if(!quick || std::find(tracked.begin(),tracked.end(),quick)!=tracked.end()) return;
    auto *native=QGuiApplication::platformNativeInterface();
    if(!native) return;
    if(!display) {
        display=static_cast<wl_display*>(native->nativeResourceForIntegration("wl_display"));
        if(!display) return;
        auto *registry=wl_display_get_registry(display);
        wl_registry_add_listener(registry,&registry_listener,nullptr);
        wl_display_roundtrip(display);
    }
    tracked.push_back(quick);
    QObject::connect(quick,&QObject::destroyed,QCoreApplication::instance(),[quick] {
        tracked.erase(std::remove(tracked.begin(),tracked.end(),quick),tracked.end());
    });
    QObject::connect(quick,&QQuickWindow::beforeFrameBegin,quick,[native,quick] {
        if(!presentation) return;
        auto *surface=static_cast<wl_surface*>(native->nativeResourceForWindow("surface",quick));
        if(!surface) return;
        auto *feedback=wp_presentation_feedback(presentation,surface);
        wp_presentation_feedback_add_listener(feedback,&feedback_listener,new Feedback {surface,generation.load()});
    },Qt::DirectConnection);
}
class ProbeFilter:public QObject {
public:explicit ProbeFilter(QObject *parent):QObject(parent) {}
    bool eventFilter(QObject *object,QEvent *event) override {
        if(event->type()==QEvent::Expose || event->type()==QEvent::PlatformSurface) attach(qobject_cast<QQuickWindow*>(object));
        return false;
    }
};
static void start_probe() {
    qmlRegisterSingletonType<TimingProbe>("BeadProbe",1,0,"TimingProbe",[](QQmlEngine*,QJSEngine*)->QObject* {return new TimingProbe;});
    auto *app=QCoreApplication::instance();
    app->installEventFilter(new ProbeFilter(app));
    QTimer::singleShot(0,app,[] {
        for(auto *window:QGuiApplication::allWindows()) attach(qobject_cast<QQuickWindow*>(window));
        auto *reporter=new QTimer(QCoreApplication::instance());
        QObject::connect(reporter,&QTimer::timeout,QCoreApplication::instance(),report);reporter->start(1000);
        std::atexit(report);
    });
}
Q_COREAPP_STARTUP_FUNCTION(start_probe)

#include "qml-presentation.moc"
