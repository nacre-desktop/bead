// SPDX-License-Identifier: GPL-3.0-only
// Measurement-only preload: Qt continues owning its connection and windows.
#include <QGuiApplication>
#include <QQuickWindow>
#include <QQuickItem>
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
#include <QKeyEvent>
#include <QMouseEvent>
struct Samples { std::vector<double> intervals; uint64_t first=0,last=0; uint32_t refresh=0,kind=0; unsigned discarded=0; };
static std::map<wl_surface*, Samples> samples;
static std::atomic<unsigned> generation=0;
static std::atomic<unsigned> input_generation=0;
static std::atomic<double> pointer_x=0,pointer_y=0;
static uint64_t stamp(clockid_t id) {timespec t{};if(clock_gettime(id,&t)!=0)return 0;return uint64_t(t.tv_sec)*1000000000+uint64_t(t.tv_nsec);}
static std::map<wl_surface*,unsigned> last_generation;
struct Feedback {wl_surface *surface;unsigned generation;bool panel;unsigned frame;};
static std::atomic<unsigned> frame_sequence=0;
class TimingProbe:public QObject {
    Q_OBJECT
public:
    Q_INVOKABLE void commit(unsigned value) {generation.store(value);}
    Q_INVOKABLE void inputCommitted() {
        if(!std::getenv("BEAD_QML_TRACE_INPUT"))return;
        const auto value=input_generation.load();if(!value)return;
        generation.store(value);
        std::fprintf(stderr,"QML_INPUT_COMMIT generation=%u monotonic_ns=%llu cpu_ns=%llu\n",value,static_cast<unsigned long long>(stamp(CLOCK_MONOTONIC)),static_cast<unsigned long long>(stamp(CLOCK_PROCESS_CPUTIME_ID)));
    }
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
    if(std::getenv("BEAD_QML_TRACE_PRESENT"))
        std::fprintf(stderr,"QML_FRAME generation=%u monotonic_ns=%llu panel=%s frame=%u\n",tag->generation,static_cast<unsigned long long>(time),tag->panel?"true":"false",tag->frame);
    if(!s.first) s.first=time;
    if(s.last && time>s.last && time>s.first+2000000000) s.intervals.push_back(double(time-s.last)/1000000);
    if(!s.last) std::fprintf(stderr,"QML_READY presentation=true\n");
    if(!last_generation.count(tag->surface) || last_generation[tag->surface]!=tag->generation) {
        std::fprintf(stderr,"QML_UI_PRESENTED generation=%u monotonic_ns=%llu cpu_ns=%llu\n",tag->generation,static_cast<unsigned long long>(time),static_cast<unsigned long long>(stamp(CLOCK_PROCESS_CPUTIME_ID)));
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
// childAt can select an opacity-zero overlay above the pointer target. Search
// visible paint branches and require the interaction's actual feedback item.
static QQuickItem *hoveredFeedback(QQuickItem *item,const QPointF &point) {
    if(!item->isVisible()||item->opacity()<=0) return nullptr;
    const auto inside=item->contains(item->mapFromScene(point));
    if(item->clip()&&!inside) return nullptr;
    if(inside&&item->property("hovered").toBool())
        for(auto *child:item->childItems())
            if(child->objectName()=="nacreInteractionFeedback") return child;
    for(auto *child:item->childItems())
        if(auto *feedback=hoveredFeedback(child,point)) return feedback;
    return nullptr;
}
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
    const auto hook=(std::getenv("BEAD_QML_TRACE_INPUT")||std::getenv("BEAD_QML_TRACE_POINTER"))?&QQuickWindow::afterSynchronizing:&QQuickWindow::beforeFrameBegin;
    QObject::connect(quick,hook,quick,[native,quick] {
        if(!presentation) return;
        auto *surface=static_cast<wl_surface*>(native->nativeResourceForWindow("surface",quick));
        if(!surface) return;
        const auto frame=frame_sequence.fetch_add(1);
        if(std::getenv("BEAD_QML_TRACE_POINTER")) {
            // afterSynchronizing holds the GUI thread while these item values are read.
            std::fprintf(stderr,"QML_POINTER_SYNC frame=%u generation=%u monotonic_ns=%llu panel=%s\n",frame,generation.load(),static_cast<unsigned long long>(stamp(CLOCK_MONOTONIC)),quick->height()>100?"true":"false");
            {
                auto *feedback=hoveredFeedback(quick->contentItem(),QPointF(pointer_x.load(),pointer_y.load()));
                std::fprintf(stderr,"QML_POINTER_HOVER frame=%u generation=%u hovered=%d opacity=%.6f\n",frame,generation.load(),feedback?1:0,feedback?feedback->opacity():-1.);
            }
            for(auto *item=quick->mouseGrabberItem();item;item=item->parentItem()) {
                if(!item->property("visualPosition").isValid()) continue;
                const auto origin=item->mapToScene(QPointF(0,0));
                std::fprintf(stderr,"QML_POINTER_VALUE frame=%u generation=%u value=%.6f position=%.6f x=%.3f y=%.3f width=%.3f padding=%.3f available=%.3f\n",frame,generation.load(),item->property("value").toDouble(),item->property("visualPosition").toDouble(),origin.x(),origin.y(),item->width(),item->property("leftPadding").toDouble(),item->property("availableWidth").toDouble());
                break;
            }
        }
        auto *feedback=wp_presentation_feedback(presentation,surface);
        wp_presentation_feedback_add_listener(feedback,&feedback_listener,new Feedback {surface,generation.load(),quick->height()>100,frame});
    },Qt::DirectConnection);
}
class ProbeFilter:public QObject {
public:explicit ProbeFilter(QObject *parent):QObject(parent) {}
    bool eventFilter(QObject *object,QEvent *event) override {
        if(event->type()==QEvent::Expose || event->type()==QEvent::PlatformSurface) attach(qobject_cast<QQuickWindow*>(object));
        if(std::getenv("BEAD_QML_TRACE_POINTER")&&qobject_cast<QQuickWindow*>(object)
            &&(event->type()==QEvent::MouseMove||event->type()==QEvent::MouseButtonPress||event->type()==QEvent::MouseButtonRelease)) {
            auto *mouse=static_cast<QMouseEvent*>(event);
            pointer_x.store(mouse->position().x());pointer_y.store(mouse->position().y());
            const auto value=input_generation.fetch_add(1)+1;
            generation.store(value);
            std::fprintf(stderr,"QML_POINTER generation=%u monotonic_ns=%llu x=%.3f y=%.3f type=%d\n",value,static_cast<unsigned long long>(stamp(CLOCK_MONOTONIC)),mouse->position().x(),mouse->position().y(),int(event->type()));
        }
        if(std::getenv("BEAD_QML_TRACE_INPUT")&&event->type()==QEvent::KeyPress&&qobject_cast<QQuickWindow*>(object)) {
            auto *key=static_cast<QKeyEvent*>(event);
            if(key->key()==Qt::Key_Backspace||(!key->text().isEmpty()&&!(key->modifiers()&(Qt::ControlModifier|Qt::AltModifier|Qt::MetaModifier)))) {
                const auto value=input_generation.fetch_add(1)+1;
                std::fprintf(stderr,"QML_INPUT_RECEIVED generation=%u monotonic_ns=%llu cpu_ns=%llu\n",value,static_cast<unsigned long long>(stamp(CLOCK_MONOTONIC)),static_cast<unsigned long long>(stamp(CLOCK_PROCESS_CPUTIME_ID)));
            }
        }
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
        QObject::connect(reporter,&QTimer::timeout,QCoreApplication::instance(),report);if(!std::getenv("BEAD_QML_QUIET_IDLE")) reporter->start(1000);
        std::atexit(report);
    });
}
Q_COREAPP_STARTUP_FUNCTION(start_probe)

#include "qml-presentation.moc"
