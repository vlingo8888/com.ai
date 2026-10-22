(ns app.view
  (:require [reagent.core :as r]))

(defn card [{:keys [title active?]}]
  [:div.flex.flex-col.gap-2.rounded-lg {:class (when active? "ring-2 ring-blue-500")}
   [:h2.text-xl.font-bold title]
   [:p {:class ["text-sm" "text-gray-500"]} "body"]
   [:span {:className "inline-block px-2"} "x"]
   [:div.w-1/2.md:w-[300px]]])
