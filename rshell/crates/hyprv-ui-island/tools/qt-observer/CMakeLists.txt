cmake_minimum_required(VERSION 3.21)
project(parity-host LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_AUTOMOC ON)
set(CMAKE_EXPORT_COMPILE_COMMANDS ON)
# QRhi (render target + readback) lives in GuiPrivate; rebuild if Qt is upgraded.
set(QT_NO_PRIVATE_MODULE_WARNING ON)

find_package(Qt6 REQUIRED COMPONENTS Gui GuiPrivate Qml Quick QuickPrivate Widgets)

add_executable(parity-host main.cpp wayland_mocks.h)
target_link_libraries(parity-host PRIVATE ${CMAKE_DL_LIBS} Qt6::Gui Qt6::GuiPrivate Qt6::Qml Qt6::Quick Qt6::QuickPrivate Qt6::Widgets)

add_executable(rect-mesh-precise rect_mesh_precise.cpp)
target_link_libraries(rect-mesh-precise PRIVATE Qt6::QuickPrivate Qt6::Gui)
